mod app;
mod commands;
mod completions;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use miette::IntoDiagnostic;
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

    /// When to colorize output
    #[arg(long, value_enum, default_value_t, global = true)]
    color: output::ColorChoice,

    /// Path to database file (default: $XDG_DATA_HOME/ranger/ranger.db)
    #[arg(long, env = "RANGER_DB", global = true)]
    db: Option<PathBuf>,

    /// Backlog to work in. `task create` adds tasks to it, `task list`
    /// lists it, task keys resolve within it first, and a bare `ranger`
    /// shows it.
    #[arg(long, env = "RANGER_DEFAULT_BACKLOG", global = true, add = ArgValueCompleter::new(completions::complete_backlog_names))]
    backlog: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(flatten)]
    Db(DbCommands),
    #[command(name = "__internal", hide = true)]
    Internal {
        #[command(subcommand)]
        command: commands::internal::InternalCommands,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Manage backlogs
    #[command(visible_alias = "b")]
    Backlog {
        #[command(subcommand)]
        command: commands::backlog::BacklogCommands,
    },
    /// Manage tasks
    #[command(visible_alias = "t")]
    Task {
        /// Space-separated filter terms: `bug` or `key:value` for a tag,
        /// `state:<state>` for a state, and a leading `-` to negate. `list`
        /// shows only tasks matching every term; `create` adds the positive
        /// tags and takes the first positive state. Task keys resolve among
        /// the matching tasks first.
        /// Example: --filter "project:ranger -bug"
        #[arg(long, global = true, allow_hyphen_values = true)]
        filter: Option<ranger::filter::Filter>,
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

fn main() -> miette::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // Handle dynamic completions before entering the tokio runtime.
    // Completers create their own single-threaded runtime to query the DB,
    // which would panic if nested inside #[tokio::main].
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let command = match cli.command {
        Some(Commands::Internal { command }) => {
            commands::internal::run(command);
            return Ok(());
        }
        Some(Commands::Db(command)) => Some(command),
        None => None,
    };

    output::init_color(cli.color);
    let db_path = resolve_db_path(cli.db);
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .into_diagnostic()?
        .block_on(async_main(db_path, cli.backlog, cli.json, command))
}

async fn async_main(
    db_path: PathBuf,
    backlog: Option<String>,
    json: bool,
    command: Option<DbCommands>,
) -> miette::Result<()> {
    let ranger = app::Ranger {
        pool: ranger::db::connect(&db_path).await?,
        backlog,
        json,
        editor: std::env::var("EDITOR").ok(),
    };

    match command {
        Some(DbCommands::Backlog { command }) => {
            commands::backlog::run(&ranger, command).await?;
        }
        Some(DbCommands::Task { filter, command }) => {
            let filter = filter.unwrap_or_default();
            commands::task::run(&ranger, command, &filter).await?;
        }
        Some(DbCommands::Comment { command }) => {
            commands::comment::run(&ranger, command).await?;
        }
        Some(DbCommands::Tag { command }) => {
            commands::tag::run(&ranger, command).await?;
        }
        Some(DbCommands::Serve { port }) => {
            commands::serve::run(&ranger, port).await?;
        }
        None => {
            // No subcommand: show the default backlog, or list them all
            let command = match &ranger.backlog {
                Some(name) => commands::backlog::BacklogCommands::Show {
                    name: name.clone(),
                    done: false,
                },
                None => commands::backlog::BacklogCommands::List,
            };
            commands::backlog::run(&ranger, command).await?;
        }
    }

    Ok(())
}
