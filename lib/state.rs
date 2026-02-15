use std::ops::Deref;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::layout::Constraint;
use ratatui::widgets::{BorderType, Borders, Paragraph, Row, StatefulWidget, Table, TableState};
use ratatui::DefaultTerminal;
use ratatui::{
    layout::Layout,
    style::Style,
    text::Line,
    widgets::{Block, Tabs, Widget},
};

use crate::tree_hoprs::{get_config_file, RepoConfig};

#[derive(Default)]
pub struct App {
    repos: Vec<String>,
    active_repo: usize,
    end: bool,
    repo_configs: Vec<RepoConfig>,
    selected_row: i16,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut app = Self::default();
        match get_config_file() {
            Ok(conf) => {
                let repos = conf.get_repos();
                app.repos = repos;
                app.active_repo = 0;
                app.end = false;
                app.repo_configs = conf.get_repo_configs();
                app.selected_row = 0;
                return Ok(app);
            }
            Err(_) => return Ok(app),
        }
    }

    pub fn render(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| frame.render_widget(self.deref(), frame.area()))?;
            self.handle_input()?;
            if self.end {
                break;
            }
        }
        Ok(())
    }

    fn handle_input(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(200)).context("event poll failed")? {
            if let Event::Key(key) = event::read().context("event read failed")? {
                match key.code {
                    KeyCode::Char('q') => self.end = true,
                    KeyCode::Char('l') => self.move_tab(1),
                    KeyCode::Char('h') => self.move_tab(-1),
                    KeyCode::Char('k') => self.move_selected(1),
                    KeyCode::Char('j') => self.move_selected(-1),
                    KeyCode::Char('d') => self.delete_worktree()?,
                    _ => (),
                }
            }
        }
        Ok(())
        // todo!()
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
            .1;
        self.repo_configs[self.active_repo].delete_worktree(to_delete)?;
        return Ok(());
    }
}

impl Widget for &App {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        use Constraint::{Fill, Length, Percentage};
        let vertical = Layout::vertical([Length(3), Fill(1), Length(4)]);
        let [tabs, body, footer] = vertical.areas(area);
        Tabs::new(self.repos.clone().into_iter())
            .block(Block::bordered().title(Line::from("TreeHoprs").centered()))
            .highlight_style(Style::default().white().on_dark_gray())
            .select(self.active_repo)
            .render(tabs, buf);
        let rows: Vec<Row> = self.repo_configs[self.active_repo]
            .list_worktrees(false)
            .context("failed to list worktrees")
            .unwrap()
            .iter()
            .map(|(path, branch)| Row::new(vec![path.to_owned(), branch.to_owned()]))
            .collect();
        let selected: usize = self.get_selcted_row(rows.len() as i16);
        let table = Table::new(rows, [Percentage(75), Percentage(25)])
            .header(Row::new(vec!["Path", "Branch"]))
            .row_highlight_style(Style::new().on_dark_gray())
            .block(
                Block::new()
                    .border_type(BorderType::Rounded)
                    .borders(Borders::ALL),
            );
        let mut table_state: TableState = TableState::new().with_selected(selected);
        StatefulWidget::render(table, body, buf, &mut table_state);

        Paragraph::new("[d] Delete | [_] Create | [_] Create new repository \n [h] Previos tab | [l] Next tab | [j] Select next | [k] Select previous | [q] Quit")
            .centered()
            .block(Block::new().borders(Borders::ALL).border_type(BorderType::Rounded))
            .render(footer, buf);
    }
}
