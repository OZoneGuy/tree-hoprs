use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Result};
use ratatui::{
    crossterm::event::{Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};
use tokio::{
    spawn,
    sync::mpsc::{channel, error::TryRecvError, Receiver},
};

use crate::ui::{
    loading::Loading,
    screen::{Screen, ScreenAction},
};

#[derive(Debug)]
pub struct CreateWorktreeScreen {
    name: String,
    loading: Arc<AtomicBool>,

    loading_signal: Option<Arc<Receiver<LoadingRes>>>,
}

impl CreateWorktreeScreen {
    pub fn new() -> Self {
        CreateWorktreeScreen {
            name: String::new(),
            loading: Arc::new(AtomicBool::new(false)),
            loading_signal: None,
        }
    }
}

#[async_trait::async_trait]
impl Screen for CreateWorktreeScreen {
    fn render(
        &self,
        _app: &crate::state::App,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) {
        use Constraint::{Length, Percentage};
        let center_area = area.centered(Length(64), Length(10));
        Block::bordered()
            .border_type(BorderType::Rounded)
            .on_dark_gray()
            .title(Line::from("Create worktree").centered())
            .render(center_area, buf);
        let [_, top, input_area] =
            Layout::vertical([Length(2), Length(3), Length(3)]).areas(center_area);
        Paragraph::new("Create worktree popup")
            .centered()
            .render(top, buf);
        let branch_name = self.name.clone();
        let __input_area = input_area.centered_horizontally(Percentage(75));
        if self.loading.load(Ordering::Relaxed) {
            Loading::render(&Loading {}, _app, __input_area, buf);
        } else {
            Paragraph::new(branch_name)
                .centered()
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().cyan()),
                )
                .render(__input_area, buf);
        }
    }

    async fn handle_input(
        &mut self,
        app: &mut crate::state::App,
        event: Event,
    ) -> Result<ScreenAction> {
        if let Some(recv) = self.loading_signal {
            match recv.try_recv() {
                // Finished action
                Ok(state) => {
                    if let LoadingRes::Ready(action) = state {
                        self.loading.store(false, Ordering::Relaxed);
                        return Ok(action);
                    }
                }
                // Still loading
                Err(TryRecvError::Empty) => return Ok(ScreenAction::Stay),
                Err(err) => {
                    return Err(anyhow!(
                        "tried reading channel after disconnect in create dialogue"
                    ))
                }
            }
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Esc => {
                    return Ok(ScreenAction::Main);
                }
                KeyCode::Char(c) => self.name.push(c),
                KeyCode::Backspace => {
                    self.name.pop();
                }
                KeyCode::Enter => {
                    let repo = app.get_active_repo().clone();
                    let branch_name = self.name.clone();
                    self.loading.store(true, Ordering::Relaxed);
                    let loadin_state = self.loading.clone();
                    spawn(async move {
                        repo.write()
                            .await
                            .create_worktree(&branch_name, true, false)
                            .await
                            .unwrap();
                        let (tx, recv) = channel::<LoadingRes>(2);
                    });
                }
                _ => (),
            }
        };
        Ok(ScreenAction::Stay)
    }
}
