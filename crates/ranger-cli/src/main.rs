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
    }

    Ok(())
}
