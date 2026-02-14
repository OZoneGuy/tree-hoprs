use std::{
    collections::HashMap,
    env::var,
    fs::{self, copy},
    io::BufRead,
    path,
    process::Command,
    str::from_utf8,
};

use path::PathBuf;

use anyhow::{anyhow, Result};
use comfy_table::Table;
use dialoguer::Input;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RepoConfig {
    repo_name: String,
    base_tree: String,
    base_path: String,
    inactive_trees: Vec<String>,
    copy_files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "repositories")]
    repo: HashMap<String, RepoConfig>,
    #[serde(rename = "active_repository")]
    active_repo: String,
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

pub fn create_worktree(mut values: RepoConfig, branch_name: String, dry_run: bool) -> Result<()> {
    let mut pull_cmd = Command::new("git");
    pull_cmd
        .current_dir(format!("{}/{}", values.base_path, values.base_tree))
        .arg("pull");
    pull_cmd.status()?;

    // Create branch if it doesn't exist
    let mut branch_cmd = Command::new("git");
    branch_cmd
        .arg("branch")
        .arg(&branch_name)
        .current_dir(format!("{}/{}", values.base_path, values.base_tree));
    if dry_run {
        println!("Would create branch {}", branch_name);
        println!("Would run command {:?}", branch_cmd);
    } else {
        branch_cmd.status()?;
    };

    // Create worktree
    let worktree_path;
    if !values.inactive_trees.is_empty() {
        worktree_path = values.inactive_trees.first().unwrap().clone();
    } else {
        let worktree_name = format!(
            "tree{}",
            fs::read_dir(&values.base_path)?
                .filter(|f| f.is_ok() && f.as_ref().unwrap().file_type().unwrap().is_dir())
                .count()
        );
        worktree_path = format!("{}/{}", values.base_path, worktree_name);
    }

    // Check if worktree already exists
    let mut worktree_cmd = Command::new("git");
    worktree_cmd.current_dir(format!("{}/{}", values.base_path, values.base_tree));

    if let Ok(worktree) = fs::read_dir(&worktree_path) {
        if worktree.count() > 0 {
            println!(
                "Worktree {} already exists, switching branch",
                worktree_path
            );
            // Switch the branch in the existing worktree
            worktree_cmd.current_dir(&worktree_path);
            worktree_cmd.arg("switch").arg(&branch_name);
        }
    } else {
        worktree_cmd
            .arg("worktree")
            .arg("add")
            .arg(&worktree_path)
            .arg(&branch_name);
    }
    if dry_run {
        println!("Would create worktree {}", &worktree_path);
        println!("Would run command {:?}", worktree_cmd);
    } else {
        worktree_cmd.status()?;

        // NOTE: There is a better way to do this. Could use pop or something :/
        if values.inactive_trees.contains(&worktree_path) {
            values.inactive_trees.remove(0);
            let mut config: Config = serde_json::from_str(&fs::read_to_string(CONFIG_FILE())?)?;
            config
                .repo
                .insert(config.active_repo.clone(), values.clone());
            fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
        }
    };

    // Copy common files from the base tree
    for file in values.copy_files {
        let mut from_path = PathBuf::new();
        from_path.push(values.base_path.clone());
        from_path.push(values.base_tree.clone());
        from_path.push(file.clone());
        let from = from_path
            .to_str()
            .ok_or(anyhow!("Failed to create from path"))?;
        let mut to_path = PathBuf::new();
        to_path.push(values.base_path.clone());
        to_path.push(values.base_tree.clone());
        to_path.push(file.clone());
        let to = to_path
            .to_str()
            .ok_or(anyhow!("Failed to create from path"))?;
        copy(from, to)?;
    }

    println!(
        "Branch {} created in worktree {}",
        &branch_name, worktree_path
    );
    Ok(())
}

pub fn list_worktrees(values: RepoConfig, raw: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("worktree")
        .arg("list")
        .current_dir(format!("{}/{}", values.base_path, values.base_tree));
    let output = cmd.output()?;

    if raw {
        for line in output.stdout.lines() {
            let items: Vec<&str> = line.as_ref().unwrap().split_whitespace().collect();
            if values.inactive_trees.contains(&items[0].to_string()) {
                continue;
            }
            println!("{}", &items[2][1..items[2].len() - 1]);
        }
    } else {
        let mut table = Table::new();
        table.set_header(["Path", "Branch"]);

        for line in output.stdout.lines() {
            let items: Vec<&str> = line.as_ref().unwrap().split_whitespace().collect();
            if values.inactive_trees.contains(&items[0].to_string()) {
                continue;
            }
            table.add_row([items[0], items[2]]);
        }
        println!("{}", table);
    }
    Ok(())
}

pub fn delete_worktree(
    mut values: RepoConfig,
    branch_names: Vec<String>,
    dry_run: bool,
) -> Result<()> {
    let mut worktree_cmd = Command::new("git");
    worktree_cmd
        .arg("worktree")
        .arg("list")
        .current_dir(format!("{}/{}", values.base_path, values.base_tree));
    let output = worktree_cmd.output()?;
    let worktrees: Vec<(String, String)> = from_utf8(&output.stdout)?
        .lines()
        .map(|line| {
            let pair = line.split_whitespace().collect::<Vec<&str>>();
            let name = {
                let mut chars = pair[2].chars();
                chars.next();
                chars.next_back();
                chars.as_str().to_string()
            };
            (pair[0].to_string(), name)
        })
        .collect();
    for branch_name in branch_names {
        let result = worktrees.iter().find(|(_, name)| name == &branch_name);
        if result.is_none() {
            println!("Worktree {} does not exist", branch_name);
            continue;
        };
        let worktree_path = &result.as_ref().unwrap().0;
        if values.inactive_trees.contains(&worktree_path) {
            println!("Worktree {} is already inactive", branch_name);
            continue;
        }
        if dry_run {
            println!("Would archive worktree {}", &worktree_path);
            continue;
        }
        values.inactive_trees.push(worktree_path.clone());
    }

    let mut config: Config = serde_json::from_str(&fs::read_to_string(CONFIG_FILE())?)?;
    config.repo.insert(values.repo_name.clone(), values);
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;

    Ok(())
}

pub fn update_main_worktree(values: RepoConfig, dry_run: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("pull")
        .current_dir(format!("{}/{}", values.base_path, values.base_tree));
    if dry_run {
        println!("Would run command {:?}", cmd);
    } else {
        cmd.status()?;
    };

    Ok(())
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
