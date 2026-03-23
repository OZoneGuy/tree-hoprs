use std::time;

use ratatui::{
    text::Line,
    widgets::{Clear, Widget},
};

use crate::ui::screen::Screen;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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
        Clear.render(area, buf);
        Line::from(SPINNER_FRAMES[index]).render(area, buf);
    }

    async fn handle_input(
        &mut self,
        _app: &mut crate::state::App,
        _event: ratatui::crossterm::event::Event,
    ) -> anyhow::Result<super::screen::ScreenAction> {
        todo!()
    }
}
