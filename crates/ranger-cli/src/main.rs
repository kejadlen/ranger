mod commands;
mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage backlogs
    Backlog {
        #[command(subcommand)]
        command: commands::backlog::BacklogCommands,
    },
    /// Manage tasks
    Task {
        #[command(subcommand)]
        command: commands::task::TaskCommands,
    },
    /// Manage comments
    Comment {
        #[command(subcommand)]
        command: commands::comment::CommentCommands,
    },
    /// Manage tags
    Tag {
        #[command(subcommand)]
        command: commands::tag::TagCommands,
    },
    /// Manage blockers
    Blocker {
        #[command(subcommand)]
        command: commands::blocker::BlockerCommands,
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
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db_path = resolve_db_path(cli.db);
    let pool = ranger_lib::db::connect(&db_path).await?;

    match cli.command {
        Commands::Backlog { command } => {
            commands::backlog::run(&pool, command, cli.json).await?;
        }
        Commands::Task { command } => {
            commands::task::run(&pool, command, cli.json).await?;
        }
        Commands::Comment { command } => {
            commands::comment::run(&pool, command, cli.json).await?;
        }
        Commands::Tag { command } => {
            commands::tag::run(&pool, command, cli.json).await?;
        }
        Commands::Blocker { command } => {
            commands::blocker::run(&pool, command, cli.json).await?;
        }
    }

    Ok(())
}
