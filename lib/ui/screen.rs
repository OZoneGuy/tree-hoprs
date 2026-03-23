use anyhow::Result;
use ratatui::{buffer::Buffer, crossterm::event::Event, layout::Rect};

use crate::state::App;

#[async_trait::async_trait]
pub(crate) trait Screen: Send {
    fn render(&self, app: &App, area: Rect, buf: &mut Buffer);
    async fn handle_input(&mut self, app: &mut App, event: Event) -> Result<ScreenAction>;
}

pub(crate) enum ScreenAction {
    Stay,
    #[allow(dead_code)]
    MoveScreen(Box<dyn Screen>),
    Main,
}
