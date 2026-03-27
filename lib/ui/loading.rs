use std::time;

use ratatui::{layout::Constraint, text::Line, widgets::Widget};

use crate::ui::screen::Screen;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug)]
pub struct Loading {}

impl Loading {}

#[async_trait::async_trait]
impl Screen for Loading {
    fn render(
        &self,
        _app: &crate::state::App,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) {
        let millis_time = time::SystemTime::now()
            .duration_since(time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let index: usize = ((millis_time as usize) / 100) % SPINNER_FRAMES.len();
        Line::from(SPINNER_FRAMES[index]).render(
            area.centered(Constraint::Length(2), Constraint::Length(2)),
            buf,
        );
    }

    async fn handle_input(
        &mut self,
        _app: &mut crate::state::App,
        _event: ratatui::crossterm::event::Event,
    ) -> anyhow::Result<super::screen::ScreenAction> {
        todo!()
    }
}
