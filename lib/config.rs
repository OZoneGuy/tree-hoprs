use std::{
    collections::HashMap,
    env::var,
    fs::{self},
};

use anyhow::{anyhow, Result};
use dialoguer::Input;
use serde::{Deserialize, Serialize};

use crate::repo_config::RepoConfig;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "repositories")]
    pub(crate) repo: HashMap<String, RepoConfig>,
    #[serde(rename = "active_repository")]
    active_repo: String,
}

impl Config {
    /// The config file path
    /// Defaults to `~/.config/tree-hoprs.json`
    #[allow(non_snake_case)]
    pub fn CONFIG_FILE() -> String {
        format!("{}/.config/tree-hoprs.json", var("HOME").unwrap())
    }

    pub fn get_config_file() -> Result<Self> {
        let config_file = fs::File::open(Self::CONFIG_FILE())?;
        let config: Self = serde_json::from_reader(config_file)?;
        return Ok(config);
    }

    pub fn create_config_file(repo: &Option<String>) -> Result<Self> {
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
        let config_file = fs::File::create(Self::CONFIG_FILE())?;
        serde_json::to_writer_pretty(config_file, &config)?;
        Ok(config)
    }

    pub fn get_values_from_config_file(&self, repo: &Option<String>) -> Result<RepoConfig> {
        if repo.is_none() {
            Ok(self.repo.get(&self.active_repo).unwrap().clone())
        } else {
            Ok(self.repo.get(repo.as_ref().unwrap()).unwrap().clone())
        }
    }

    pub fn get_repos(&self) -> Vec<String> {
        self.repo.keys().map(|s| s.to_owned()).collect()
    }
    pub fn get_repo_configs(&self) -> Vec<RepoConfig> {
        self.repo.values().map(|c| c.to_owned()).collect()
    }

    pub fn set_active_repo(&mut self, repo_name: String) -> std::result::Result<(), anyhow::Error> {
        self.active_repo = repo_name;
        fs::write(Self::CONFIG_FILE(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add_repo(
        &mut self,
        repo_name: String,
        base_tree: String,
        base_path: String,
    ) -> Result<()> {
        if self.repo.contains_key(&repo_name) {
            return Err(anyhow!("Repository already exists"));
        }
        self.repo.insert(
            repo_name.clone(),
            RepoConfig {
                repo_name: repo_name.clone(),
                base_tree,
                base_path,
                inactive_trees: Vec::new(),
                copy_files: Vec::new(),
            },
        );
        fs::write(Self::CONFIG_FILE(), serde_json::to_string_pretty(&self)?)?;
        Ok(())
    }

    pub fn delete_repo(&mut self, repo_name: String) -> Result<()> {
        if !self.repo.contains_key(&repo_name) {
            return Err(anyhow!(
                "Repository not found. Available repositories are {:?}",
                self.repo.keys()
            ));
        }
        self.repo.remove(&repo_name);
        fs::write(Self::CONFIG_FILE(), serde_json::to_string_pretty(&self)?)?;
        Ok(())
    }
}
