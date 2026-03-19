use anyhow::Result;
use ratatui::{buffer::Buffer, crossterm::event::Event, layout::Rect};

use crate::state::App;

pub(crate) trait Screen {
    fn render(&self, app: &App, area: Rect, buf: &mut Buffer);
    fn handle_input(&mut self, app: &mut App, event: Event) -> Result<ScreenAction>;
}

pub(crate) enum ScreenAction {
    Stay,
    #[allow(dead_code)]
    MoveScreen(Box<dyn Screen>),
    Main,
}
