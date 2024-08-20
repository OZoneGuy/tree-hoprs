use std::collections::HashMap;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
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
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all the worktrees
    List,
    /// Create a new worktree
    Create { name: String },
    /// Archive a worktree
    Delete { name: String },
    /// Update a worktree
    Update { name: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RepoConfig {
    base_branch: String,
    base_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    repo: HashMap<String, RepoConfig>,
    active_repo: String,
}

fn main() {
    println!("Hello, world!");
}
