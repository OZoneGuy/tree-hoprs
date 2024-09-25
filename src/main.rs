use std::{collections::HashMap, env::var, fs, io::BufRead, process::Command, str::from_utf8};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use comfy_table::Table;
use dialoguer::Input;
use serde::{Deserialize, Serialize};

/// The config file path
/// Defaults to `~/.config/tree-hoprs.json`
#[allow(non_snake_case)]
fn CONFIG_FILE() -> String {
    format!("{}/.config/tree-hoprs.json", var("HOME").unwrap())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
    /// Don't actually do anything, just print the commands
    #[arg(short, long)]
    dry_run: bool,

    /// The base branch to use
    #[arg(short = 'b', long = "base")]
    base_branch: Option<String>,
    /// The base path to use
    #[arg(short = 'p', long = "path")]
    base_path: Option<String>,
    /// The config file to use
    #[arg(short = 'r', long = "repo")]
    repo: Option<String>,

    /// The command to run
    #[command(subcommand)]
    command: TreeCommand,
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {}

#[derive(Subcommand, Debug)]
enum TreeCommand {
    /// List all the worktrees
    List {
        #[arg(short, long)]
        raw: bool,
    },
    /// Create a new worktree
    Create {
        branch_name: String,
    },
    /// Archive a worktree
    Delete {
        branch_name: String,
    },
    /// Update a worktree
    Update,
    /// Set a config value
    SetRepo {
        repo_name: String,
    },
    /// Add a new repository
    #[command(name = "add-repo")]
    AddRepo {
        repo_name: String,
        base_tree: String,
        base_path: String,
    },
    GetRepos,
    /// Delete a repository
    DeleteRepo {
        repo_name: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct RepoConfig {
    base_tree: String,
    base_path: String,
    inactive_trees: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    #[serde(rename = "repositories")]
    repo: HashMap<String, RepoConfig>,
    #[serde(rename = "active_repository")]
    active_repo: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.verbose {
        dbg!(&args);
    }

    // Used across the program to pass the configuration
    let mut values = RepoConfig {
        base_tree: args.base_branch.unwrap_or(String::new()),
        base_path: args.base_path.unwrap_or(String::new()),
        inactive_trees: Vec::new(),
    };

    // Check if optional values are passed
    if values.base_tree.is_empty() || values.base_path.is_empty() || args.repo.is_none() {
        // Try to read the config file
        match get_values_from_config_file(&args.repo) {
            Ok(v) => {
                values = v;
            }
            Err(_) => {
                println!("Config file not found or invalid, creating new config file");
                create_config_file(&mut values, &args.repo)?;
            }
        }

        if args.verbose {
            dbg!(&values);
        };
    };

    match args.command {
        TreeCommand::List { raw } => list_worktrees(values, raw),
        TreeCommand::Create { branch_name: name } => {
            println!("Creating worktree {}", name);
            create_worktree(values, name, args.dry_run)
        }
        TreeCommand::Delete { branch_name: name } => {
            println!("Deleting worktree {}", name);
            delete_worktree(values, name, args.dry_run)
        }
        TreeCommand::Update => {
            println!("Updating base worktree");
            update_main_worktree(values, args.dry_run)
        }
        TreeCommand::SetRepo { repo_name } => {
            println!("Setting config value");
            set_active_repo(repo_name)
        }
        TreeCommand::AddRepo {
            repo_name,
            base_tree,
            base_path,
        } => {
            println!("Adding repository");
            add_repo(repo_name, base_tree, base_path)
        }
        TreeCommand::DeleteRepo { repo_name } => {
            println!("Deleting repository");
            delete_repo(repo_name)
        }
        TreeCommand::GetRepos => get_repos(),
    }
}

fn set_active_repo(repo_name: String) -> std::result::Result<(), anyhow::Error> {
    let mut config = get_config_file()?;
    config.active_repo = repo_name;
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

fn get_repos() -> std::result::Result<(), anyhow::Error> {
    let config = get_config_file()?;
    for (repo_name, _) in config.repo.iter() {
        println!("{}", repo_name);
    }
    Ok(())
}

fn get_config_file() -> Result<Config> {
    let config_file = fs::File::open(CONFIG_FILE())?;
    let config: Config = serde_json::from_reader(config_file)?;
    return Ok(config);
}

fn add_repo(repo_name: String, base_tree: String, base_path: String) -> Result<()> {
    let mut config = get_config_file()?;
    if config.repo.contains_key(&repo_name) {
        return Err(anyhow!("Repository already exists"));
    }
    config.repo.insert(
        repo_name.clone(),
        RepoConfig {
            base_tree,
            base_path,
            inactive_trees: Vec::new(),
        },
    );
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

fn delete_repo(repo_name: String) -> Result<()> {
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

fn create_config_file(values: &mut RepoConfig, repo: &Option<String>) -> Result<()> {
    let base_tree = Input::new()
        .with_prompt("Base tree name")
        .default(values.base_tree.clone())
        .interact_text()?;
    let base_path = Input::new()
        .with_prompt("Base repos path")
        .default(values.base_path.clone())
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
    values.base_tree = base_tree;
    values.base_path = base_path;
    config.repo.insert(repo_name, values.clone());
    let config_file = fs::File::create(CONFIG_FILE())?;
    serde_json::to_writer_pretty(config_file, &config)?;
    Ok(())
}

fn get_values_from_config_file(repo: &Option<String>) -> Result<RepoConfig> {
    let config_file = fs::File::open(CONFIG_FILE())?;
    let config: Config = serde_json::from_reader(config_file)?;
    if repo.is_none() {
        Ok(config.repo.get(&config.active_repo).unwrap().clone())
    } else {
        Ok(config.repo.get(repo.as_ref().unwrap()).unwrap().clone())
    }
}

fn create_worktree(mut values: RepoConfig, branch_name: String, dry_run: bool) -> Result<()> {
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
    // XXX: Should fail if branch already exists
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
            config.repo.insert(config.active_repo.clone(), values);
            fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;
        }
    };

    println!(
        "Branch {} created in worktree {}",
        &branch_name, worktree_path
    );
    Ok(())
}

fn list_worktrees(values: RepoConfig, raw: bool) -> Result<()> {
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

fn delete_worktree(mut values: RepoConfig, branch_name: String, dry_run: bool) -> Result<()> {
    let mut worktree_cmd = Command::new("git");
    worktree_cmd
        .arg("worktree")
        .arg("list")
        .current_dir(format!("{}/{}", values.base_path, values.base_tree));
    let output = worktree_cmd.output()?;
    let result = from_utf8(&output.stdout)?
        .lines()
        .find(|&line| line.to_string().contains(&branch_name))
        .map(|line| line.to_string());
    if result.is_none() {
        println!("Worktree {} does not exist", branch_name);
        return Ok(());
    }
    let worktree_path = result.unwrap().split_whitespace().collect::<Vec<&str>>()[0].to_string();
    let mut config: Config = serde_json::from_str(&fs::read_to_string(CONFIG_FILE())?)?;
    if config
        .repo
        .get(&config.active_repo)
        .unwrap()
        .inactive_trees
        .contains(&worktree_path)
    {
        println!("Worktree {} is already inactive", branch_name);
        return Ok(());
    }

    if dry_run {
        println!("Would archive worktree {}", &worktree_path);
        return Ok(());
    }

    // Get config from file
    values.inactive_trees.push(worktree_path);
    config.repo.insert(config.active_repo.clone(), values);
    fs::write(CONFIG_FILE(), serde_json::to_string_pretty(&config)?)?;

    Ok(())
}

fn update_main_worktree(values: RepoConfig, dry_run: bool) -> Result<()> {
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
