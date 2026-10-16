mod cli;
mod commands;
mod output;

use miette::IntoDiagnostic;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use cli::Command;

fn resolve_db_path(cli_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = cli_path {
        return path;
    }
    if let Ok(path) = std::env::var("RANGER_DB") {
        return PathBuf::from(path);
    }
    let xdg = xdg::BaseDirectories::with_prefix("ranger").expect("failed to resolve XDG dirs");
    xdg.place_data_file("ranger.db")
        .expect("failed to create data directory")
}

fn main() -> miette::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = match cli::parse(lexopt::Parser::from_env()) {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("Run 'ranger --help' for usage.");
            std::process::exit(2);
        }
    };

    output::init_color(cli.globals.color);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .into_diagnostic()?
        .block_on(async_main(cli))
}

async fn async_main(cli: cli::Cli) -> miette::Result<()> {
    let json = cli.globals.json;
    let db_path = resolve_db_path(cli.globals.db);
    let pool = ranger::db::connect(&db_path).await?;

    match cli.command {
        Some(Command::Backlog(command)) => {
            commands::backlog::run(&pool, command, json).await?;
        }
        Some(Command::Task(command)) => {
            commands::task::run(&pool, command, json).await?;
        }
        Some(Command::Comment(command)) => {
            commands::comment::run(&pool, command, json).await?;
        }
        Some(Command::Tag(command)) => {
            commands::tag::run(&pool, command, json).await?;
        }
        Some(Command::Serve { port, backlog }) => {
            commands::serve::run(&pool, port, backlog).await?;
        }
        None => {
            // No subcommand: show the default backlog
            match cli::default_backlog() {
                Some(name) => {
                    let show_cmd = commands::backlog::BacklogCommands::Show { name, done: false };
                    commands::backlog::run(&pool, show_cmd, json).await?;
                }
                None => {
                    // No default backlog set — list all backlogs
                    let list_cmd = commands::backlog::BacklogCommands::List;
                    commands::backlog::run(&pool, list_cmd, json).await?;
                }
            }
        }
    }

    Ok(())
}
