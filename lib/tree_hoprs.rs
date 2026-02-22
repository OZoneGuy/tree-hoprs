use std::{
    collections::HashMap,
    env::var,
    fs::{self, copy},
    path::{self, Path},
};

use git2::{
    build::CheckoutBuilder, Cred, FetchOptions, RemoteCallbacks, Repository, WorktreeAddOptions,
};
use path::PathBuf;

use anyhow::{anyhow, Context, Result};
use dialoguer::Input;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RepoConfig {
    repo_name: String,
    base_tree: String,
    base_path: String,
    inactive_trees: Vec<String>,
    copy_files: Vec<String>,
}

impl RepoConfig {
    pub fn get_inactive_trees(&self) -> &Vec<String> {
        &self.inactive_trees
    }

    /// Returns the list of worktrees for a given repo
    ///
    /// Calls `git worktree list` on the main path of the repository and retrns a vector of pairs of
    /// strings. The first item is the worktree path, and the second item is the branch name.
    pub fn list_worktrees(&self, include_inactive: bool) -> Result<Vec<WorktreeListing>> {
        // Get listt of worktrees
        let repo = Repository::open(format!("{}/{}", self.base_path, self.base_tree))?;
        let mut trees = Vec::new();

        // filter out worktrees in inactive list
        for tree_name in repo.worktrees()?.iter() {
            let worktree = repo.find_worktree(tree_name.unwrap())?;
            let path = worktree.path().to_str().unwrap().to_owned();
            if !include_inactive && self.inactive_trees.contains(&path) {
                continue;
            }
            let r = Repository::open_from_worktree(&repo.find_worktree(tree_name.unwrap())?)?;
            let head = r.head()?;
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
            trees.push(WorktreeListing {
                path,
                reference: head.name().unwrap().to_owned(),
                state: WorktreeState {
                    pr_state: PrState::Open,
                    local_state,
                },
            });
        }
        Ok(trees)
    }

    pub fn delete_worktree(&mut self, branch_name: &str) -> Result<()> {
        let worktrees = self.list_worktrees(false)?;
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

        let mut config: Config = serde_json::from_str(&fs::read_to_string(CONFIG_FILE())?)?;
        config.repo.insert(self.repo_name.clone(), self.clone());
        fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;

        Ok(())
    }

    pub fn create_worktree(
        &mut self,
        branch_name: &String,
        _silent: bool,
        _dry_run: bool,
    ) -> Result<(String, String)> {
        self.update_main_worktree(false)?;

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
            let mut config: Config = serde_json::from_str(&fs::read_to_string(CONFIG_FILE())?)?;
            config.repo.insert(config.active_repo.clone(), self.clone());
            fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
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

    pub fn update_main_worktree(&self, _dry_run: bool) -> Result<()> {
        let repo = Repository::open(format!("{}/{}", self.base_path, self.base_tree))?;
        let remotes = repo.remotes()?;
        let remote = remotes.get(0).ok_or(Errors::NoRemote)?;
        let mut rcb = RemoteCallbacks::new();
        rcb.credentials(|_u, user, _types| {
            Cred::ssh_key(
                user.unwrap(),
                None,
                Path::new(&format!("{}/.ssh/github", env!("HOME"))),
                None,
            )
        });
        let mut remote_obj = repo.find_remote(remote)?;
        remote_obj
            .fetch(
                &[&self.base_tree],
                Some(FetchOptions::new().remote_callbacks(rcb)),
                None,
            )
            .context("Feching remote main branch")?;
        let remote_ref = repo.find_reference(&format!("refs/heads/{}", self.base_tree))?;
        repo.set_head(remote_ref.name().ok_or(Errors::NoRemote)?)?;
        repo.checkout_head(Some(CheckoutBuilder::default().force()))?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "repositories")]
    repo: HashMap<String, RepoConfig>,
    #[serde(rename = "active_repository")]
    active_repo: String,
}

impl Config {
    pub fn get_repos(&self) -> Vec<String> {
        self.repo.keys().map(|s| s.to_owned()).collect()
    }
    pub fn get_repo_configs(&self) -> Vec<RepoConfig> {
        self.repo.values().map(|c| c.to_owned()).collect()
    }
}

pub enum PrState {
    Loading,
    Closed,
    Open,
    Failing,
    Requested,
    Merged,
}

pub enum LocalState {
    Clean,
    Changes,
    Staged,
}

pub struct WorktreeState {
    pub pr_state: PrState,
    pub local_state: LocalState,
}

pub struct WorktreeListing {
    pub path: String,
    pub reference: String,
    pub state: WorktreeState,
}

/// The config file path
/// Defaults to `~/.config/tree-hoprs.json`
#[allow(non_snake_case)]
pub fn CONFIG_FILE() -> String {
    format!("{}/.config/tree-hoprs.json", var("HOME").unwrap())
}

pub fn set_active_repo(repo_name: String) -> std::result::Result<(), anyhow::Error> {
    let mut config = get_config_file()?;
    config.active_repo = repo_name;
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn get_repos() -> std::result::Result<(), anyhow::Error> {
    let config = get_config_file()?;
    for (repo_name, _) in config.repo.iter() {
        println!("{}", repo_name);
    }
    Ok(())
}

pub fn get_config_file() -> Result<Config> {
    let config_file = fs::File::open(CONFIG_FILE())?;
    let config: Config = serde_json::from_reader(config_file)?;
    return Ok(config);
}

pub fn add_repo(repo_name: String, base_tree: String, base_path: String) -> Result<()> {
    let mut config = get_config_file()?;
    if config.repo.contains_key(&repo_name) {
        return Err(anyhow!("Repository already exists"));
    }
    config.repo.insert(
        repo_name.clone(),
        RepoConfig {
            repo_name: repo_name.clone(),
            base_tree,
            base_path,
            inactive_trees: Vec::new(),
            copy_files: Vec::new(),
        },
    );
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn delete_repo(repo_name: String) -> Result<()> {
    let mut config = get_config_file()?;
    if !config.repo.contains_key(&repo_name) {
        return Err(anyhow!(
            "Repository not found. Available repositories are {:?}",
            config.repo.keys()
        ));
    }
    config.repo.remove(&repo_name);
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn create_config_file(repo: &Option<String>) -> Result<RepoConfig> {
    let base_tree = Input::new().with_prompt("Base tree name").interact_text()?;
    let base_path = Input::new()
        .with_prompt("Base repos path")
        .interact_text()?;
    let repo_name: String;
    if repo.is_none() {
        repo_name = Input::new().with_prompt("Repo name").interact_text()?;
    } else {
        repo_name = repo.clone().unwrap();
    }
    let mut config = Config {
        repo: HashMap::new(),
        active_repo: repo_name.clone(),
    };
    let values = RepoConfig {
        repo_name: repo_name.clone(),
        base_tree,
        base_path,
        inactive_trees: Vec::new(),
        copy_files: Vec::new(),
    };
    config.repo.insert(repo_name.clone(), values.clone());
    let config_file = fs::File::create(CONFIG_FILE())?;
    serde_json::to_writer_pretty(config_file, &config)?;
    Ok(values)
}

pub fn get_values_from_config_file(repo: &Option<String>) -> Result<RepoConfig> {
    let config_file = fs::File::open(CONFIG_FILE())?;
    let config: Config = serde_json::from_reader(config_file)?;
    if repo.is_none() {
        Ok(config.repo.get(&config.active_repo).unwrap().clone())
    } else {
        Ok(config.repo.get(repo.as_ref().unwrap()).unwrap().clone())
    }
}

pub fn add_file(mut values: RepoConfig, file_path: &str) -> Result<()> {
    // check that file exists
    let mut full_path = PathBuf::new();
    full_path.push(values.base_tree.clone());
    full_path.push(values.base_path.clone());
    full_path.push(file_path);

    let path_string = full_path
        .to_str()
        .ok_or(anyhow!("Failed to create the file path"))?;

    if !full_path.exists() {
        println!("File does not exist at {}", path_string);
        return Err(anyhow!("File does not exist"));
    }
    values.copy_files.push(path_string.into());

    let mut config = get_config_file()?;
    config.repo.insert(values.repo_name.clone(), values);
    Ok(())
}
