mod commands;
mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser)]
#[command(name = "ranger", about = "Personal task tracker")]
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
    /// Start the web server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 3000)]
        port: u16,
        /// Backlog to display
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: String,
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

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

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
        Some(Commands::Serve { port, backlog }) => {
            commands::serve::run(&pool, port, backlog).await?;
        }
        None => {
            // No subcommand: show the default backlog
            let backlog_name = std::env::var("RANGER_DEFAULT_BACKLOG").ok();
            match backlog_name {
                Some(name) => {
                    let show_cmd = commands::backlog::BacklogCommands::Show { name };
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
