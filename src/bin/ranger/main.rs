mod commands;
mod completions;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Build-time version set by build.rs (dev) or release workflow.
const VERSION: &str = env!("RANGER_VERSION");

#[derive(Parser)]
#[command(name = "ranger", version = VERSION, about = "Personal task tracker")]
struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to database file (default: $XDG_DATA_HOME/ranger/ranger.db)
    #[arg(long, env = "RANGER_DB", global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage backlogs
    #[command(visible_alias = "b")]
    Backlog {
        #[command(subcommand)]
        command: commands::backlog::BacklogCommands,
    },
    /// Manage tasks
    #[command(visible_alias = "t")]
    Task {
        #[command(subcommand)]
        command: commands::task::TaskCommands,
    },
    /// Manage comments
    #[command(visible_alias = "c")]
    Comment {
        #[command(subcommand)]
        command: commands::comment::CommentCommands,
    },
    /// Manage tags
    #[command(visible_alias = "g")]
    Tag {
        #[command(subcommand)]
        command: commands::tag::TagCommands,
    },
    /// Start the web server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 3000)]
        port: u16,
        /// Default backlog to display
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG", add = ArgValueCompleter::new(completions::complete_backlog_names))]
        backlog: Option<String>,
    },
}

fn resolve_db_path(cli_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = cli_path {
        return path;
    }
    let xdg = xdg::BaseDirectories::with_prefix("ranger").expect("failed to resolve XDG dirs");
    xdg.place_data_file("ranger.db")
        .expect("failed to create data directory")
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // Handle dynamic completions before entering the tokio runtime.
    // Completers create their own single-threaded runtime to query the DB,
    // which would panic if nested inside #[tokio::main].
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    let db_path = resolve_db_path(cli.db);
    let pool = ranger::db::connect(&db_path).await?;

    match cli.command {
        Some(Commands::Backlog { command }) => {
            commands::backlog::run(&pool, command, cli.json).await?;
        }
        Some(Commands::Task { command }) => {
            commands::task::run(&pool, command, cli.json).await?;
        }
        Some(Commands::Comment { command }) => {
            commands::comment::run(&pool, command, cli.json).await?;
        }
        Some(Commands::Tag { command }) => {
            commands::tag::run(&pool, command, cli.json).await?;
        }
        Some(Commands::Serve { port, backlog }) => {
            commands::serve::run(&pool, port, backlog).await?;
        }
        None => {
            // No subcommand: show the default backlog
            let backlog_name = std::env::var("RANGER_DEFAULT_BACKLOG").ok();
            match backlog_name {
                Some(name) => {
                    let show_cmd = commands::backlog::BacklogCommands::Show { name, done: false };
                    commands::backlog::run(&pool, show_cmd, cli.json).await?;
                }
                None => {
                    // No default backlog set — list all backlogs
                    let list_cmd = commands::backlog::BacklogCommands::List;
                    commands::backlog::run(&pool, list_cmd, cli.json).await?;
                }
            }
        }
    }

    Ok(())
}
