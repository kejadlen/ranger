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

    /// The default backlog. Task keys resolve within it first, and a bare
    /// `ranger` shows it. Normally set through the env var.
    #[arg(long, env = "RANGER_DEFAULT_BACKLOG", global = true, hide = true)]
    default_backlog: Option<String>,

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

fn main() -> miette::Result<()> {
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
        .build()
        .into_diagnostic()?
        .block_on(async_main())
}

async fn async_main() -> miette::Result<()> {
    let cli = Cli::parse();
    output::init_color(cli.color);
    let db_path = resolve_db_path(cli.db);
    let ranger = app::Ranger {
        pool: ranger::db::connect(&db_path).await?,
        default_backlog: cli.default_backlog,
        json: cli.json,
        editor: std::env::var("EDITOR").ok(),
    };

    match cli.command {
        Some(Commands::Backlog { command }) => {
            commands::backlog::run(&ranger, command).await?;
        }
        Some(Commands::Task { filter, command }) => {
            let filter = filter.unwrap_or_default();
            commands::task::run(&ranger, command, &filter).await?;
        }
        Some(Commands::Comment { command }) => {
            commands::comment::run(&ranger, command).await?;
        }
        Some(Commands::Tag { command }) => {
            commands::tag::run(&ranger, command).await?;
        }
        Some(Commands::Serve { port, backlog }) => {
            commands::serve::run(&ranger, port, backlog).await?;
        }
        None => {
            // No subcommand: show the default backlog, or list them all
            let command = match &ranger.default_backlog {
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
