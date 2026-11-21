mod app;
mod commands;
mod completions;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
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

    /// Filter terms to work within, usually set through the env var.
    /// `--filter` narrows it, and an empty value matches every task.
    /// Example: RANGER_DEFAULT_FILTER=project:ranger
    #[arg(
        long,
        env = "RANGER_DEFAULT_FILTER",
        global = true,
        allow_hyphen_values = true
    )]
    default_filter: Option<ranger::filter::Filter>,

    /// Deprecated: the backlog whose `project:` tag is the default filter
    /// when RANGER_DEFAULT_FILTER isn't set.
    #[arg(long, env = "RANGER_DEFAULT_BACKLOG", global = true, hide = true)]
    default_backlog: Option<String>,

    /// Space-separated filter terms: `bug` or `key:value` for a tag,
    /// `state:<state>` for a state, and a leading `-` to negate. Together
    /// with the default filter, `task list` shows only tasks matching every
    /// term, `task create` adds the positive tags and takes the first
    /// positive state, `tag list` shows the tags matching tasks use, and
    /// task keys resolve among matching tasks first.
    /// Example: --filter "project:ranger -bug"
    #[arg(long, global = true, allow_hyphen_values = true)]
    filter: Option<ranger::filter::Filter>,

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
    let (default_filter, warning) =
        app::default_filter(cli.default_filter, cli.default_backlog.as_deref());
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    let filter = default_filter.and(cli.filter.unwrap_or_default());
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .into_diagnostic()?
        .block_on(async_main(db_path, filter, cli.json, command))
}

async fn async_main(
    db_path: PathBuf,
    filter: ranger::filter::Filter,
    json: bool,
    command: Option<DbCommands>,
) -> miette::Result<()> {
    let ranger = app::Ranger {
        pool: ranger::db::connect(&db_path).await?,
        filter,
        json,
        editor: std::env::var("EDITOR").ok(),
    };

    match command {
        Some(DbCommands::Backlog { command }) => {
            commands::backlog::run(&ranger, command).await?;
        }
        Some(DbCommands::Task { command }) => {
            commands::task::run(&ranger, command).await?;
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
            // No subcommand: list the tasks the filter matches.
            let command = commands::task::TaskCommands::List {
                state: None,
                tag: None,
                archived: false,
            };
            commands::task::run(&ranger, command).await?;
        }
    }

    Ok(())
}
