use anyhow::Result;
use clap::{Parser, Subcommand};

use comfy_table::Table;
use lib::repo_config::RepoConfig;
use lib::state::App;
use lib::tree_hoprs::*;
use lib::{config::Config, gh::GitHub};
use log::{debug, error, info, trace, warn, LevelFilter};
use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use shellexpand::tilde;
use simplelog::{ColorChoice, ConfigBuilder, TermLogger, TerminalMode, WriteLogger};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Verbose output
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbosity: u8,
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

    Authenticate,
}

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    let mut config_builder = ConfigBuilder::new();
    config_builder
        .set_location_level(LevelFilter::Debug)
        .set_time_level(LevelFilter::Off);

    let args = Args::parse();

    let level: LevelFilter;
    match args.verbosity {
        0 => level = LevelFilter::Info,
        1 => level = LevelFilter::Debug,
        _ => level = LevelFilter::Trace,
    };

    if args.command.is_none() {
        let log_path = tilde("~/.local/share/tree-hoprs.log");
        let log_path = std::path::Path::new(log_path.as_ref());
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !log_path.exists() {
            std::fs::write(log_path, "")?;
        }
        let rotate_file = BasicRollingFileAppender::new(
            log_path,
            RollingConditionBasic::new().max_size(1_000_000),
            1,
        )?;
        WriteLogger::init(level, config_builder.build(), rotate_file)?;
        debug!("args = {:?}", &args);
        trace!("starting the application");
        let mut app = App::new()?;
        trace!("Getting the terminal object");
        let terminal = ratatui::init();
        trace!("started the tui");
        let app_res = app.render(terminal).await;
        ratatui::restore();
        return app_res;
    }
    TermLogger::init(
        level,
        config_builder.build(),
        TerminalMode::Stderr,
        ColorChoice::Always,
    )?;
    debug!("args = {:?}", &args);

    trace!("loading config");
    let mut config: Config = match Config::get_config_file() {
        Ok(c) => c,
        Err(_) => Config::create_config_file(&args.repo)?,
    };

    // Used across the program to pass the configuration
    let mut repo_config: RepoConfig = config.get_values_from_config_file(&args.repo)?;

    debug!("using config: {:?}", &repo_config);

    match args.command.unwrap() {
        TreeCommand::List { raw } => {
            let worktrees = repo_config.list_worktrees(false).await?;
            if raw {
                debug!("listing wortrees in raw mode");
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
            info!("Creating worktree {}", name);
            let (branch_name, worktree_path) = repo_config
                .create_worktree(&name, false, args.dry_run)
                .await?;
            info!(
                "Branch {} created in worktree {}",
                branch_name, worktree_path
            );
            Ok(())
        }
        TreeCommand::Delete { branch_names } => {
            info!("Deleting worktres:");
            for name in &branch_names {
                info!("{}", name);
            }
            for branch in branch_names {
                match repo_config.delete_worktree(&branch).await {
                    Ok(_) => info!("Deleted branch: {}", branch),
                    Err(e) => match e.downcast_ref::<Errors>() {
                        Some(Errors::WorktreeInactive { worktree }) => {
                            warn!("Worktree already inactive: {}", worktree)
                        }
                        Some(Errors::WorktreeDoesNotExist { worktree }) => {
                            warn!("Worktree does not exist {}", worktree);
                        }
                        Some(Errors::NoRemote) => error!("Remote does not exist"),
                        None => return Err(e),
                    },
                };
            }
            Ok(())
        }
        TreeCommand::Update => {
            info!("Updating base worktree");
            repo_config.update_main_worktree(args.dry_run).await
        }
        TreeCommand::SetRepo { repo_name } => {
            info!("Setting config value");
            config.set_active_repo(repo_name)
        }
        TreeCommand::AddRepo {
            repo_name,
            base_tree,
            base_path,
        } => {
            info!("Adding repository");
            config.add_repo(repo_name, base_tree, base_path)
        }
        TreeCommand::DeleteRepo { repo_name } => {
            info!("Deleting repository");
            config.delete_repo(repo_name)
        }
        TreeCommand::GetRepos => {
            config.get_repos();
            Ok(())
        }
        TreeCommand::AddFile { file_path } => repo_config.add_file(&file_path),
        TreeCommand::Authenticate => {
            let token = GitHub::setup_auth()?;
            config.add_auth_token(token)?;
            Ok(())
        }
    }
}
