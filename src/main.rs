use anyhow::Result;
use clap::{Parser, Subcommand};

use comfy_table::Table;
use lib::config::Config;
use lib::repo_config::RepoConfig;
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

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.verbose {
        dbg!(&args);
    }

    if args.command.is_none() {
        let mut app = App::new()?;
        let terminal = ratatui::init();
        let app_res = app.render(terminal).await;
        ratatui::restore();
        return app_res;
    }

    let mut config: Config = match Config::get_config_file() {
        Ok(c) => c,
        Err(_) => Config::create_config_file(&args.repo)?,
    };

    // Used across the program to pass the configuration
    let mut repo_config: RepoConfig = config.get_values_from_config_file(&args.repo)?;

    if args.verbose {
        dbg!(&repo_config);
    };

    match args.command.unwrap() {
        TreeCommand::List { raw } => {
            let worktrees = repo_config.list_worktrees(false)?;
            if raw {
                for tree in worktrees {
                    println!("{}", &tree.reference);
                }
            } else {
                let mut table = Table::new();
                table.set_header(["Path", "Branch"]);

                for tree in worktrees {
                    table.add_row([tree.path, tree.reference]);
                }
                println!("{}", table);
            }

            Ok(())
        }
        TreeCommand::Create { branch_name: name } => {
            println!("Creating worktree {}", name);
            let (branch_name, worktree_path) = repo_config
                .create_worktree(&name, false, args.dry_run)
                .await?;
            println!(
                "Branch {} created in worktree {}",
                branch_name, worktree_path
            );
            Ok(())
        }
        TreeCommand::Delete { branch_names } => {
            println!("Deleting worktres:");
            for name in &branch_names {
                println!("{}", name);
            }
            for branch in branch_names {
                match repo_config.delete_worktree(&branch) {
                    Ok(_) => println!("Deleted branch: {}", branch),
                    Err(e) => match e.downcast_ref::<Errors>() {
                        Some(Errors::WorktreeInactive { worktree }) => {
                            println!("Worktree already inactive: {}", worktree)
                        }
                        Some(Errors::WorktreeDoesNotExist { worktree }) => {
                            println!("Worktree does not exist {}", worktree);
                        }
                        Some(Errors::NoRemote) => println!("Remote does not exist"),
                        None => return Err(e),
                    },
                };
            }
            Ok(())
        }
        TreeCommand::Update => {
            println!("Updating base worktree");
            repo_config.update_main_worktree(args.dry_run).await
        }
        TreeCommand::SetRepo { repo_name } => {
            println!("Setting config value");
            config.set_active_repo(repo_name)
        }
        TreeCommand::AddRepo {
            repo_name,
            base_tree,
            base_path,
        } => {
            println!("Adding repository");
            config.add_repo(repo_name, base_tree, base_path)
        }
        TreeCommand::DeleteRepo { repo_name } => {
            println!("Deleting repository");
            config.delete_repo(repo_name)
        }
        TreeCommand::GetRepos => {
            config.get_repos();
            Ok(())
        }
        TreeCommand::AddFile { file_path } => repo_config.add_file(&file_path),
    }
}
