use std::{
    fs::{self, copy},
    path::{self, Path},
};

use git2::{build::CheckoutBuilder, Repository, WorktreeAddOptions};
use path::PathBuf;
use tokio::{process::Command, task::spawn_blocking};

use crate::config::Config;
use crate::tree_hoprs::Errors;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Configuration for a specific repository with its worktrees and settings.
///
/// Stores information about a repository including its base tree, location, and worktrees.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RepoConfig {
    /// The name identifier for this repository
    pub(crate) repo_name: String,
    /// The name of the main/base worktree
    pub(crate) base_tree: String,
    /// The base path where the repository and its worktrees are stored
    pub(crate) base_path: String,
    /// List of inactive worktree paths (worktrees that have been deleted but paths are preserved for reuse)
    pub(crate) inactive_trees: Vec<String>,
    /// List of files to be copied when creating new worktrees
    pub(crate) copy_files: Vec<String>,
}

impl RepoConfig {
    /// Returns a reference to the list of inactive worktree paths
    pub fn get_inactive_trees(&self) -> &Vec<String> {
        &self.inactive_trees
    }

    /// Returns the list of worktrees for a given repo
    ///
    /// Calls `git worktree list` on the main path of the repository and retrns a vector of pairs of
    /// strings. The first item is the worktree path, and the second item is the branch name.
    pub async fn list_worktrees(&self, include_inactive: bool) -> Result<Vec<WorktreeListing>> {
        // Get listt of worktrees
        let base_path = format!("{}/{}", self.base_path, self.base_tree);
        let inactive_trees = self.inactive_trees.clone();
        spawn_blocking(move || {
            let repo = Repository::open(&base_path)?;
            let mut trees = Vec::new();

            // Add the base tree
            trees.push(create_listing_from_repo(&repo, base_path)?);

            // filter out worktrees in inactive list
            for tree_name in repo.worktrees()?.iter() {
                let worktree = repo.find_worktree(tree_name.unwrap())?;
                let path = worktree.path().to_str().unwrap().to_owned();
                if !include_inactive && inactive_trees.contains(&path) {
                    continue;
                }
                let worktree_repo =
                    Repository::open_from_worktree(&repo.find_worktree(tree_name.unwrap())?)?;
                trees.push(create_listing_from_repo(&worktree_repo, path)?);
            }
            Ok(trees)
        })
        .await?
    }

    /// Marks a worktree as inactive by moving it to the inactive_trees list
    ///
    /// Finds the worktree by branch name and adds its path to the inactive list.
    /// Updates the configuration file to persist the changes.
    pub async fn delete_worktree(&mut self, branch_name: &str) -> Result<()> {
        let worktrees = self.list_worktrees(false).await?;
        let result = worktrees
            .iter()
            .find(|listing| listing.reference == branch_name);
        if result.is_none() {
            return Err(Errors::WorktreeDoesNotExist {
                worktree: branch_name.to_owned(),
            }
            .into());
        };
        let worktree_path = &result.as_ref().unwrap().path;
        if self.inactive_trees.contains(&worktree_path) {
            return Err(Errors::WorktreeInactive {
                worktree: branch_name.to_owned(),
            }
            .into());
        }
        self.inactive_trees.push(worktree_path.clone());

        let mut config: Config = serde_json::from_str(&fs::read_to_string(Config::CONFIG_FILE())?)?;
        config.repo.insert(self.repo_name.clone(), self.clone());
        fs::write(
            Config::CONFIG_FILE(),
            serde_json::to_string_pretty(&config)?,
        )?;

        Ok(())
    }

    /// Creates a new worktree for the given branch name
    ///
    /// Updates the main worktree, creates a new branch if needed, and sets up the worktree.
    /// Reuses inactive worktree paths if available. Copies configured files to the new worktree.
    pub async fn create_worktree(
        &mut self,
        branch_name: &String,
        _silent: bool,
        _dry_run: bool,
    ) -> Result<(String, String)> {
        self.update_main_worktree(false).await?;

        let repo = Repository::open(format!("{}/{}", self.base_path, self.base_tree))?;
        // Create branch if it doesn't exist
        let head_commit = repo.head()?.peel_to_commit()?;
        let branch = repo.branch(branch_name, &head_commit, true)?;

        // Create worktree
        let worktree_path;
        if !self.inactive_trees.is_empty() {
            worktree_path = self.inactive_trees.first().unwrap().clone();
        } else {
            let worktree_name = format!(
                "tree{}",
                fs::read_dir(&self.base_path)?
                    .filter(|f| f.is_ok() && f.as_ref().unwrap().file_type().unwrap().is_dir())
                    .count()
            );
            worktree_path = format!("{}/{}", self.base_path, worktree_name);
        }

        // Switch the branch in the existing worktree
        if let Ok(worktree) = fs::read_dir(&worktree_path) {
            if worktree.count() > 0 {
                // Switch the branch in the existing worktree
                let _repo = Repository::open(&worktree_path)?;
                _repo
                    .set_head(branch.get().name().unwrap())
                    .context("setting head in existing dir")?;
                _repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
            }
        } else {
            repo.worktree(
                branch_name,
                Path::new(&worktree_path),
                Some(
                    WorktreeAddOptions::new()
                        .checkout_existing(true)
                        .reference(Some(&branch.into_reference())),
                ),
            )?;
        }

        // NOTE: There is a better way to do this. Could use pop or something :/
        if self.inactive_trees.contains(&worktree_path) {
            self.inactive_trees.remove(0);
            let mut config: Config =
                serde_json::from_str(&fs::read_to_string(Config::CONFIG_FILE())?)?;
            config.repo.insert(self.repo_name.clone(), self.clone());
            fs::write(
                Config::CONFIG_FILE(),
                serde_json::to_string_pretty(&config)?,
            )?;
        }

        // Copy common files from the base tree
        for file in &self.copy_files {
            let mut from_path = PathBuf::new();
            from_path.push(self.base_path.clone());
            from_path.push(self.base_tree.clone());
            from_path.push(file.clone());
            let from = from_path
                .to_str()
                .ok_or(anyhow!("Failed to create from path"))?;
            let mut to_path = PathBuf::new();
            to_path.push(self.base_path.clone());
            to_path.push(self.base_tree.clone());
            to_path.push(file.clone());
            let to = to_path
                .to_str()
                .ok_or(anyhow!("Failed to create from path"))?;
            copy(from, to)?;
        }
        Ok((branch_name.to_owned(), worktree_path))
    }

    /// Updates the main worktree by pulling the latest changes
    ///
    /// Runs `git pull` on the base tree directory.
    pub async fn update_main_worktree(&self, _dry_run: bool) -> Result<()> {
        Command::new("git")
            .arg("pull")
            .current_dir(format!("{}/{}", self.base_path, self.base_tree))
            .output()
            .await?;
        Ok(())
    }

    /// Adds a file to the list of files to be copied when creating new worktrees
    ///
    /// Verifies that the file exists at the specified path and adds it to the copy_files list.
    /// Updates the configuration file to persist the changes.
    /// XXX: This most likely does not behave as intended. Does not save the data to disk.
    pub fn add_file(&mut self, file_path: &str) -> Result<()> {
        // check that file exists
        let mut full_path = PathBuf::new();
        full_path.push(self.base_tree.clone());
        full_path.push(self.base_path.clone());
        full_path.push(file_path);

        let path_string = full_path
            .to_str()
            .ok_or(anyhow!("Failed to create the file path"))?;

        if !full_path.exists() {
            println!("File does not exist at {}", path_string);
            return Err(anyhow!("File does not exist"));
        }
        self.copy_files.push(path_string.into());
        Ok(())
    }
}

/// Creates a WorktreeListing from a git repository
///
/// Extracts repository information including the current branch reference and the state of
/// uncommitted/staged changes. Determines if there are working directory changes, staged changes,
/// or if the worktree is clean.
///
/// # Arguments
///
/// * `repo` - A reference to the git2 Repository
/// * `path` - The file system path of the worktree
///
/// # Returns
///
/// A Result containing a WorktreeListing with the repository's path, current branch reference,
/// and local state (Clean, Changes, or Staged). The PR state is initialized as Loading.
fn create_listing_from_repo(repo: &Repository, path: String) -> Result<WorktreeListing> {
    let mut listing: WorktreeListing = WorktreeListing::default();
    listing.path = path;
    listing.reference = repo.head()?.shorthand().unwrap().to_owned();
    let local_state = {
        use git2::Status;
        let is_changed = repo.statuses(None)?.iter().any(|entry| {
            entry.status().contains(
                Status::WT_NEW
                    | Status::WT_RENAMED
                    | Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_TYPECHANGE,
            )
        });
        let is_staged = repo.statuses(None)?.iter().any(|entry| {
            entry.status().contains(
                Status::INDEX_NEW
                    | Status::INDEX_RENAMED
                    | Status::INDEX_DELETED
                    | Status::INDEX_MODIFIED
                    | Status::INDEX_TYPECHANGE,
            )
        });
        if is_changed {
            LocalState::Changes
        } else if is_staged {
            LocalState::Staged
        } else {
            LocalState::Clean
        }
    };
    listing.state = WorktreeState {
        pr_state: PrState::Loading,
        local_state,
    };
    return Ok(listing);
}

/// Represents the state of a pull request associated with a worktree
#[derive(Default)]
pub enum PrState {
    /// Pull request is currently being loaded
    #[default]
    Loading,
    /// Pull request has been closed
    Closed,
    /// Pull request is open
    Open,
    /// Pull request checks are failing
    Failing,
    /// Review has been requested
    Requested,
    /// Pull request has been merged
    Merged,
}

/// Represents the local state of changes in a worktree
#[derive(Default)]
pub enum LocalState {
    /// No uncommitted changes
    #[default]
    Clean,
    /// Uncommitted changes exist in the working directory
    Changes,
    /// Changes have been staged for commit
    Staged,
}

/// Combines pull request and local state information for a worktree
#[derive(Default)]
pub struct WorktreeState {
    /// The state of the associated pull request
    pub pr_state: PrState,
    /// The local git state of the worktree
    pub local_state: LocalState,
}

/// Information about a worktree including its path, branch, and state
#[derive(Default)]
pub struct WorktreeListing {
    /// The file system path where the worktree is located
    pub path: String,
    /// The branch name or reference currently checked out in the worktree
    pub reference: String,
    /// The combined state of the worktree (PR and local changes)
    pub state: WorktreeState,
}
