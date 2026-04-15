use log::*;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;

use anyhow::{anyhow, Result};
use crossterm::event::{self, Event, KeyCode};
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
        trace!("Getting the config file for the tui");
        match Config::get_config_file() {
            Ok(conf) => {
                let (tx, recv) = channel(4);
                let repos = conf.get_repos();
                trace!("creating app object");
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
        let input_tx = self.transmitter.clone();
        let end_signal = Arc::new(AtomicBool::new(false));

        let input_end_signal = end_signal.clone();
        // Background task to trigger render ticks and user input events
        spawn(async move {
            loop {
                if input_end_signal.load(Ordering::Relaxed) {
                    break;
                };
                if event::poll(Duration::from_millis(50)).unwrap() {
                    let user_event = event::read();
                    if let Ok(e) = user_event {
                        input_tx.send(AppEvent::Input(e)).await.unwrap();
                    }
                }
            }
        });

        loop {
            select! {
                Some(e) = self.input_channel.recv() => {
                match e {
                    AppEvent::Input(input_event) => {
                        let mut new_event = input_event;
                        loop{
                            self.handle_input(new_event).await?;
                            if event::poll(Duration::from_millis(0)).unwrap() {
                                new_event = event::read()?;
                            } else {
                                break;
                            }
                        };
                    },
                    AppEvent::Quit => {
                        info!("ending tui loop");
                        end_signal.store(true, Ordering::Relaxed);
                    }
                    AppEvent::SwitchScreen(action) => match action {
                        ScreenAction::Main => self.active_screen = None,
                        ScreenAction::MoveScreen(new_screen) => {
                            self.active_screen = Some(new_screen)
                        }
                        ScreenAction::Stay => (),
                    },
                    AppEvent::UpdateRepo(new_config) => {
                        trace!(
                            "acquiring write lock on repo_config[{}] for update repo event",
                            self.active_repo
                        );
                        self.repo_configs[self.active_repo] = Arc::new(RwLock::new(new_config));
                        trace!(
                            "released write lock on repo_config[{}] after update",
                            self.active_repo
                        );
                    }
                    AppEvent::Tick => (),
                };
                },
                _ = sleep(Duration::from_millis(TICK_RATE)) => (),
            };
            let widget = AppWidget::from_app(self).await?;
            terminal.draw(|frame| frame.render_widget(&widget, frame.area()))?;
            if end_signal.load(Ordering::Relaxed) {
                info!("quitting now?");
                // event_loop.abort();
                return Ok(());
            }
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
                        trace!("quitting while loading");
                        info!("quitting");
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
                        info!("triggering quit");
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
        trace!("move tab by {}", direction);
        let n = (self.active_repo as i32) + direction;
        trace!("calculated index: {}", n);

        // Use rem_euclid for proper modulo with negative numbers
        let repos_len = self.repos.len() as i32;
        self.active_repo = n.rem_euclid(repos_len) as usize;
        self.selected_row = 0;

        trace!("moved to repo index: {}", self.active_repo);
    }

    fn move_selected(&mut self, direction: isize) {
        trace!("moving selected row by {}", direction);
        self.selected_row += direction;
        trace!("new selected row: {}", self.selected_row);
    }

    pub fn get_selected_row(&self, worktree_count: isize) -> usize {
        trace!("calculating wrapped row index for count: {worktree_count}");
        let wrapped =
            (((self.selected_row % worktree_count) + worktree_count) % worktree_count) as usize;
        trace!(
            "wrapped row index: {wrapped} (from raw: {})",
            self.selected_row
        );
        wrapped
    }

    async fn delete_worktree(&mut self) -> Result<()> {
        info!("deleting worktree");
        trace!(
            "acquiring read lock on repo_config[{}] to list worktrees",
            self.active_repo
        );
        let work_trees = self.repo_configs[self.active_repo]
            .read()
            .await
            .list_worktrees(false)?;
        trace!(
            "released read lock on repo_config[{}] after listing worktrees",
            self.active_repo
        );
        let to_delete = &work_trees
            .get(self.get_selected_row(work_trees.len() as isize))
            .ok_or(anyhow!("IOOB when selecting worktree"))?
            .reference;
        trace!(
            "acquiring write lock on repo_config[{}] to delete worktree '{}'",
            self.active_repo,
            to_delete
        );
        self.repo_configs[self.active_repo]
            .write()
            .await
            .delete_worktree(to_delete)
            .await?;
        trace!(
            "released write lock on repo_config[{}] after deleting worktree",
            self.active_repo
        );
        return Ok(());
    }

    fn create_worktree(&mut self) -> Result<()> {
        trace!("creating worktree screen");
        self.active_screen = Some(Box::new(CreateWorktreeScreen::new(
            self.transmitter.clone(),
        )));
        Ok(())
    }

    fn update_mainworktree(&self) -> Result<()> {
        info!("updating main worktree");
        let repo = self.repo_configs[self.active_repo].clone();
        let state = self.state.clone();
        trace!("creating sender for update async call");
        // let sender = self.transmitter.clone();
        let active_repo_idx = self.active_repo;
        trace!("setting loading state to true");
        state.store(AppState::Loading as u8, Ordering::Relaxed);
        spawn(async move {
            trace!(
                "acquiring read lock on repo_config[{}] to update main worktree",
                active_repo_idx
            );
            repo.read().await.update_main_worktree(false).await.unwrap();
            trace!(
                "released read lock on repo_config[{}] after updating main worktree",
                active_repo_idx
            );
            // sender.send(AppEvent::Tick).await.unwrap();
            trace!("setting main loading state to false");
            state.store(AppState::Normal as u8, Ordering::Relaxed);
        });
        Ok(())
    }
}
