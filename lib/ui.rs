use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Direction, Layout},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    DefaultTerminal, Frame,
};

pub fn render(mut terminal: DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(draw)?;
        if should_quite()? {
            break;
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());
    let text = Paragraph::new("top").block(
        Block::new()
            .borders(Borders::ALL)
            .title(Line::from("top").centered()),
    );
    let bot_text = Paragraph::new("bottom")
        .block(Block::new().borders(Borders::ALL).title("Botton paragraph"));
    frame.render_widget(text, layout[0]);
    frame.render_widget(bot_text, layout[1]);
}

fn should_quite() -> Result<bool> {
    if event::poll(Duration::from_millis(200)).context("event poll failed")? {
        if let Event::Key(key) = event::read().context("event read failed")? {
            return Ok(KeyCode::Char('q') == key.code);
        }
    }
    Ok(false)
}

