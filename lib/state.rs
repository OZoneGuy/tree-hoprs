use std::ops::Deref;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use anyhow::{anyhow, Context, Result};
use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use ratatui::layout::{Constraint, Spacing};
use ratatui::style::{Styled, Stylize};
use ratatui::symbols::merge::MergeStrategy;
use ratatui::widgets::{BorderType, Borders, Paragraph, Row, StatefulWidget, Table, TableState};
use ratatui::DefaultTerminal;
use ratatui::{
    layout::Layout,
    style::Style,
    text::Line,
    widgets::{Block, Tabs, Widget},
};
use tokio::time::sleep;
use tokio::{select, spawn};

use crate::config::Config;
use crate::repo_config::RepoConfig;
use crate::ui::create_worktree::CreateWorktreeScreen;
use crate::ui::loading::Loading;
use crate::ui::screen::Screen;

#[repr(u8)]
pub(crate) enum AppState {
    Normal = 0,
    Loading = 1,
    Quitting = 2,
}

pub struct App {
    pub(crate) active_screen: Option<Box<dyn Screen>>,

    input_channel: Receiver<Event>,

    pub(crate) state: Arc<AtomicU8>,
    repos: Vec<String>,
    active_repo: usize,
    repo_configs: Vec<RepoConfig>,
    selected_row: i16,
}

impl App {
    fn start_input_pooling(send: Sender<Event>) {
        spawn(async move {
            let mut reader = EventStream::new();
            loop {
                if let Some(Ok(e)) = reader.next().await {
                    // XXX: Handle result
                    send.send(e).await.unwrap();
                }
            }
        });
    }

    pub fn new() -> Result<Self> {
        match Config::get_config_file() {
            Ok(conf) => {
                let (send, recv) = channel(2);
                let repos = conf.get_repos();
                let app = Self {
                    active_screen: None,

                    input_channel: recv,

                    state: Arc::new(AtomicU8::new(AppState::Normal as u8)),
                    repos,
                    active_repo: 0,
                    repo_configs: conf.get_repo_configs(),
                    selected_row: 0,
                };
                App::start_input_pooling(send);
                return Ok(app);
            }
            Err(e) => return Err(anyhow!("Failed to read the config: {}", e)),
        }
    }

    pub fn get_active_repo(&mut self) -> Arc<&RepoConfig> {
        Arc::new(self.repo_configs.get(self.active_repo).unwrap())
    }

    pub async fn render(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            if AppState::Quitting as u8 == self.state.as_ref().load(Ordering::Relaxed) {
                println!("quitting");
                return Ok(());
            }
            terminal.draw(|frame| frame.render_widget(self.deref(), frame.area()))?;
            select! {
                res = self.input_channel.recv() => {
                    if let Some(event) = res {
                        self.handle_input(event).await?;
                    }
                },
                _ = sleep(Duration::from_millis(20)) => {},
            }
        }
    }

    /// Handles user input based on state.
    ///
    /// Handle user input based on the controls shown in the UI.
    /// There are two modes. `Normal` and `CreateWorktree`.
    /// Polls input every 50ms
    async fn handle_input(&mut self, user_event: Event) -> Result<()> {
        // If loading block all input, except for quitting
        if AppState::Loading as u8 == self.state.as_ref().load(Ordering::Relaxed) {
            if let Event::Key(key) = user_event {
                match key.code {
                    KeyCode::Char('q') => {
                        self.state
                            .as_ref()
                            .store(AppState::Quitting as u8, Ordering::Relaxed);
                    }
                    _ => (),
                }
            }
            return Ok(());
        }

        // If in another screen. Let it capture the input.
        if let Some(mut screen) = self.active_screen.take() {
            let action = screen.handle_input(self, user_event).await?;
            use crate::ui::screen::ScreenAction::*;
            match action {
                Main => self.active_screen = None,
                Stay => self.active_screen = Some(screen),
                MoveScreen(s) => self.active_screen = Some(s),
            }
        } else {
            // If not in another screen, then handle the main input.
            if let Event::Key(key) = user_event {
                match key.code {
                    KeyCode::Char('q') => {
                        self.state
                            .as_ref()
                            .store(AppState::Quitting as u8, Ordering::Relaxed);
                    }
                    KeyCode::Char('l') => self.move_tab(1),
                    KeyCode::Char('h') => self.move_tab(-1),
                    KeyCode::Char('k') => self.move_selected(-1),
                    KeyCode::Char('j') => self.move_selected(1),
                    KeyCode::Char('d') => self.delete_worktree()?,
                    KeyCode::Char('c') => self.create_worktree()?,
                    KeyCode::Char('u') => self.update_mainworktree()?,
                    _ => (),
                }
            }
        }
        Ok(())
    }

    fn move_tab(&mut self, direction: i32) {
        let n = (self.active_repo as i32) + direction;
        if n < 0 {
            self.active_repo = (n + (self.repos.len() as i32)) as usize;
        }
        self.selected_row = 0;
        self.active_repo = (n as usize) % self.repos.len()
    }

    fn move_selected(&mut self, direction: i16) {
        self.selected_row += direction;
    }

    fn get_selcted_row(&self, worktree_count: i16) -> usize {
        (((self.selected_row % worktree_count) + worktree_count) % worktree_count) as usize
    }

    fn delete_worktree(&mut self) -> Result<()> {
        let work_trees = self.repo_configs[self.active_repo].list_worktrees(false)?;
        let to_delete = &work_trees
            .get(self.get_selcted_row(work_trees.len() as i16))
            .ok_or(anyhow!("IOOB when selecting worktree"))?
            .reference;
        self.repo_configs[self.active_repo].delete_worktree(to_delete)?;
        return Ok(());
    }

    fn create_worktree(&mut self) -> Result<()> {
        self.active_screen = Some(Box::new(CreateWorktreeScreen::default()));
        Ok(())
    }

    fn update_mainworktree(&self) -> Result<()> {
        let repo = self.repo_configs[self.active_repo].clone();
        let state = self.state.clone();
        spawn(async move {
            state.store(AppState::Loading as u8, Ordering::Relaxed);
            repo.clone().update_main_worktree(false).await.unwrap();
            state.store(AppState::Normal as u8, Ordering::Relaxed);
        });
        Ok(())
    }
}

impl Widget for &App {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        use Constraint::{Fill, Length, Percentage};
        let vertical =
            Layout::vertical([Length(3), Fill(1), Length(4)]).spacing(Spacing::Overlap(1));
        let [tabs, body, footer] = vertical.areas(area);
        Block::bordered()
            .border_type(BorderType::Rounded)
            .merge_borders(MergeStrategy::Exact)
            .title(
                Line::from(vec!["TreeHoprs ".bold(), env!("CARGO_PKG_VERSION").bold()]).centered(),
            )
            .render(tabs, buf);
        let [tab_internal] = Layout::vertical(vec![Percentage(100)])
            .margin(1)
            .areas(tabs);
        Tabs::new(self.repos.clone().into_iter())
            .style(Style::default().gray())
            .highlight_style(Style::default().cyan().bold())
            .select(self.active_repo)
            .render(tab_internal, buf);
        let rows: Vec<Row> = self.repo_configs[self.active_repo]
            .list_worktrees(false)
            .context("failed to list worktrees")
            .unwrap()
            .iter()
            .map(|listing| {
                use crate::repo_config::LocalState::*;
                let local_state_icon = match listing.state.local_state {
                    Clean => "".set_style(Style::default().green()),
                    Staged => "".set_style(Style::default().yellow()),
                    Changes => "".set_style(Style::default().red()),
                };
                Row::new(vec![
                    listing.reference.clone().into(),
                    listing.path.clone().into(),
                    local_state_icon,
                ])
            })
            .collect();
        let selected: usize = self.get_selcted_row(rows.len() as i16);
        let table = Table::new(rows, [Fill(2), Fill(4), Fill(1)])
            .header(Row::new(vec!["Branch", "Path", "Local state"]).bold())
            .row_highlight_style(Style::new().italic().blue())
            .highlight_symbol(">> ")
            .block(
                Block::new()
                    .border_type(BorderType::Rounded)
                    .merge_borders(MergeStrategy::Fuzzy)
                    .borders(Borders::ALL),
            );
        let mut table_state: TableState = TableState::new().with_selected(selected);
        StatefulWidget::render(table, body, buf, &mut table_state);

        let hint_style = Style::new().bold();
        Paragraph::new(vec![
            Line::from(vec![
                "[d] Delete".set_style(hint_style),
                " | ".into(),
                "[c] Create".set_style(hint_style),
                " | ".into(),
                "[u] Update".set_style(hint_style),
                " | ".into(),
                "[_] Create new repository".set_style(hint_style).dim(),
            ]),
            Line::from(vec![
                "[h/l] Switch tabs".set_style(hint_style),
                " | ".into(),
                "[j/k] Select".set_style(hint_style),
                " | ".into(),
                "[q] Quit".set_style(hint_style),
            ]),
        ])
        .centered()
        .block(
            Block::new()
                .borders(Borders::ALL)
                .merge_borders(MergeStrategy::Fuzzy)
                .border_type(BorderType::Rounded),
        )
        .render(footer, buf);

        if let Some(box_screen) = &self.active_screen.as_deref() {
            box_screen.render(self, area, buf);
        };

        if AppState::Loading as u8 == self.state.load(Ordering::Acquire) {
            let loading_area = area.centered(Constraint::Length(10), Constraint::Length(10));
            Loading::render(&Loading {}, self, loading_area, buf)
        }
    }
}
