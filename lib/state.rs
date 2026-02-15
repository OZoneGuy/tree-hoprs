use std::ops::Deref;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::layout::Constraint;
use ratatui::widgets::{StatefulWidget, Table, TableState};
use ratatui::DefaultTerminal;
use ratatui::{
    layout::Layout,
    style::Style,
    text::Line,
    widgets::{Block, Tabs, Widget},
};

use crate::tree_hoprs::{get_config_file, RepoConfig};

pub struct App {
    repos: Vec<String>,
    active_repo: usize,
    end: bool,
    repo_configs: Vec<RepoConfig>,
}

impl App {
    pub fn new() -> Result<Self> {
        match get_config_file() {
            Ok(conf) => {
                dbg!("read config successfully");
                let repos = conf.get_repos();
                return Ok(Self {
                    repos: repos,
                    active_repo: 0,
                    end: false,
                    repo_configs: conf.get_repo_configs(),
                });
            }
            Err(e) => {
                dbg!("reading error: {:}", e);
                return Ok(Self {
                    repos: vec![],
                    active_repo: 0,
                    end: false,
                    repo_configs: vec![],
                });
            }
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
                    _ => (),
                }
            }
        }
        Ok(())
        // todo!()
    }

    fn move_tab(&mut self, arg: i32) {
        let n = (self.active_repo as i32) + arg;
        if n < 0 {
            self.active_repo = (n + (self.repos.len() as i32)) as usize;
        }
        self.active_repo = (n as usize) % self.repos.len()
    }
}

impl Widget for &App {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        use Constraint::{Fill, Length};
        let vertical = Layout::vertical([Length(3), Fill(1), Length(4)]);
        let [tabs, body, _footer] = vertical.areas(area);
        Tabs::new(self.repos.clone().into_iter())
            .block(Block::bordered().title(Line::from("TreeHoprs").centered()))
            .highlight_style(Style::default().white().on_dark_gray())
            .select(self.active_repo)
            .render(tabs, buf);
        let table: Table = todo!();
        let mut table_state: TableState = todo!();
        StatefulWidget::render(table, body, buf, &mut table_state);
    }
}
