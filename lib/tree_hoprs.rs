use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors {
    #[error("Worktree does not exist {worktree:?}")]
    WorktreeDoesNotExist { worktree: String },
    #[error("Worktree is inactive: {worktree:?}")]
    WorktreeInactive { worktree: String },
    #[error("No remote defined")]
    NoRemote,
}
