use anyhow::Result;
use clap::{Parser, Subcommand};

use lib::state::App;
use lib::tree_hoprs::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
    /// Don't actually do anything, just print the commands
    #[arg(short, long)]
    dry_run: bool,

    #[arg(short = 'r', long = "repo")]
    repo: Option<String>,

    /// The command to run
    #[command(subcommand)]
    command: Option<TreeCommand>,
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
        branch_names: Vec<String>,
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
    #[command(name = "add-file")]
    AddFile {
        file_path: String,
    },
}
fn main() -> Result<()> {
    let args = Args::parse();
    if args.verbose {
        dbg!(&args);
    }

    if args.command.is_none() {
        let mut app = App::new()?;
        let terminal = ratatui::init();
        let app_res = app.render(terminal);
        ratatui::restore();
        return app_res;
    }

    // Used across the program to pass the configuration
    let values: RepoConfig;

    // Try to read the config file
    match get_values_from_config_file(&args.repo) {
        Ok(v) => {
            values = v;
        }
        Err(_) => {
            println!("Config file not found or invalid, creating new config file");
            values = create_config_file(&args.repo)?;
        }
    }

    if args.verbose {
        dbg!(&values);
    };

    match args.command.unwrap() {
        TreeCommand::List { raw } => list_worktrees(values, raw),
        TreeCommand::Create { branch_name: name } => {
            println!("Creating worktree {}", name);
            create_worktree(values, name, args.dry_run)
        }
        TreeCommand::Delete { branch_names } => {
            println!("Deleting worktres:");
            for name in &branch_names {
                println!("{}", name);
            }
            delete_worktree(values, branch_names, args.dry_run)
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
        TreeCommand::AddFile { file_path } => add_file(values, &file_path),
    }
}
