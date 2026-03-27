use std::fmt::Debug;

use anyhow::Result;
use ratatui::{buffer::Buffer, crossterm::event::Event, layout::Rect};

use crate::state::App;

#[async_trait::async_trait]
pub trait Screen: Send + Sync + Debug {
    fn render(&self, app: &App, area: Rect, buf: &mut Buffer);
    async fn handle_input(&mut self, app: &mut App, event: Event) -> Result<ScreenAction>;
}

#[derive(Debug)]
pub enum ScreenAction {
    Stay,
    #[allow(dead_code)]
    MoveScreen(Box<dyn Screen>),
    Main,
}
