use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;

use anyhow::{anyhow, Result};
use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::time::sleep;
use tokio::{select, spawn};

use crate::config::Config;
use crate::repo_config::RepoConfig;
use crate::ui::create_worktree::CreateWorktreeScreen;
use crate::ui::main::AppWidget;
use crate::ui::screen::{Screen, ScreenAction};

#[repr(u8)]
pub enum AppState {
    Normal,
    Loading,
    Quitting,
}

impl AppState {
    pub fn from_u8(n: u8) -> Result<Self> {
        match n {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Loading),
            2 => Ok(Self::Quitting),
            _ => Err(anyhow!("u8 outside of enum range")),
        }
    }
}

/// The application tick rate in ms.
const TICK_RATE: u64 = 50;

/// An enum to send events to the main controll, `App`, to trigger application events. All events
/// should trigger a redraw of the screen. Sent to the main controller via an mpsc Transmitter.
#[derive(Debug)]
pub enum AppEvent {
    /// Normal periodic tick to draw the screen.
    Tick,
    /// Input events.
    Input(Event),
    /// Switch active screen
    SwitchScreen(ScreenAction),
    /// Update the active repoconfig.
    UpdateRepo(RepoConfig),
    /// Quit signal
    Quit,
}

type SyncRepoConfig = Arc<RwLock<RepoConfig>>;

pub struct App {
    pub active_screen: Option<Box<dyn Screen>>,

    /// The transmitter. Passed to other components to pass events to the main app
    pub transmitter: Sender<AppEvent>,
    /// Internal reciever. The receiver for `transmitter`. Not passed and read internally only.
    input_channel: Receiver<AppEvent>,

    pub state: Arc<AtomicU8>,
    pub repos: Vec<String>,
    pub active_repo: usize,
    pub repo_configs: Vec<SyncRepoConfig>,
    pub selected_row: isize,
}

impl App {
    pub fn new() -> Result<Self> {
        match Config::get_config_file() {
            Ok(conf) => {
                let (tx, recv) = channel(4);
                let repos = conf.get_repos();
                let app = Self {
                    active_screen: None,

                    input_channel: recv,
                    transmitter: tx,

                    state: Arc::new(AtomicU8::new(AppState::Normal as u8)),
                    repos,
                    active_repo: 0,
                    repo_configs: conf
                        .get_repo_configs()
                        .iter()
                        .map(|c| Arc::new(RwLock::new(c.clone())))
                        .collect(),
                    selected_row: 0,
                };
                return Ok(app);
            }
            Err(e) => return Err(anyhow!("Failed to read the config: {}", e)),
        }
    }

    pub fn get_active_repo(&self) -> SyncRepoConfig {
        self.repo_configs.get(self.active_repo).unwrap().clone()
    }

    pub async fn render(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let ticker_tx = self.transmitter.clone();

        // Background task to trigger render ticks and user input events
        spawn(async move {
            let mut reader = EventStream::new();
            loop {
                select! {
                    user_event = reader.next() => {
                        if let Some(Ok(e)) = user_event {
                            ticker_tx.send(AppEvent::Input(e)).await.unwrap();
                        }
                    },
                    _ = sleep(Duration::from_millis(TICK_RATE)) => ticker_tx.send(AppEvent::Tick).await.unwrap(),
                };
            }
        });

        loop {
            if let Some(e) = self.input_channel.recv().await {
                match e {
                    AppEvent::Input(input_event) => self.handle_input(input_event).await?,
                    AppEvent::Quit => {
                        return Ok(());
                    }
                    AppEvent::SwitchScreen(action) => match action {
                        ScreenAction::Main => self.active_screen = None,
                        ScreenAction::MoveScreen(new_screen) => {
                            self.active_screen = Some(new_screen)
                        }
                        ScreenAction::Stay => (),
                    },
                    AppEvent::UpdateRepo(new_config) => {
                        self.repo_configs[self.active_repo] = Arc::new(RwLock::new(new_config))
                    }
                    AppEvent::Tick => (),
                };
                let widget = AppWidget::from_app(self).await?;
                terminal.draw(|frame| frame.render_widget(&widget, frame.area()))?;
            };
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
                        self.transmitter.send(AppEvent::Quit).await.unwrap();
                    }
                    KeyCode::Char('l') => self.move_tab(1),
                    KeyCode::Char('h') => self.move_tab(-1),
                    KeyCode::Char('k') => self.move_selected(-1),
                    KeyCode::Char('j') => self.move_selected(1),
                    KeyCode::Char('d') => self.delete_worktree().await?,
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

    fn move_selected(&mut self, direction: isize) {
        self.selected_row += direction;
    }

    pub fn get_selcted_row(&self, worktree_count: isize) -> usize {
        (((self.selected_row % worktree_count) + worktree_count) % worktree_count) as usize
    }

    async fn delete_worktree(&mut self) -> Result<()> {
        let work_trees = self.repo_configs[self.active_repo]
            .read()
            .await
            .list_worktrees(false)
            .await?;
        let to_delete = &work_trees
            .get(self.get_selcted_row(work_trees.len() as isize))
            .ok_or(anyhow!("IOOB when selecting worktree"))?
            .reference;
        self.repo_configs[self.active_repo]
            .write()
            .await
            .delete_worktree(to_delete)
            .await?;
        return Ok(());
    }

    fn create_worktree(&mut self) -> Result<()> {
        self.active_screen = Some(Box::new(CreateWorktreeScreen::new(
            self.transmitter.clone(),
        )));
        Ok(())
    }

    fn update_mainworktree(&self) -> Result<()> {
        let repo = self.repo_configs[self.active_repo].clone();
        let state = self.state.clone();
        let sender = self.transmitter.clone();
        state.store(AppState::Loading as u8, Ordering::Relaxed);
        spawn(async move {
            repo.read().await.update_main_worktree(false).await.unwrap();
            sender.send(AppEvent::Tick).await.unwrap();
            state.store(AppState::Normal as u8, Ordering::Relaxed);
        });
        Ok(())
    }
}
