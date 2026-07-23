use lexopt::prelude::*;
use ranger::db::SqlitePool;
use ranger::error::RangerError;
use ranger::ops;

use crate::cli::{self, Globals, OwnedArg};
use crate::output;

const HELP: &str = "\
Manage comments

Usage: ranger comment [OPTIONS] <COMMAND>

Commands:
  add <TASK> <BODY>  Add a comment to a task [alias: a]
  list <TASK>        List comments on a task [alias: ls]

TASK may be any unique task key prefix.

Options:
      --json          Output as JSON
      --color <WHEN>  When to colorize output [auto|always|never]
      --db <PATH>     Path to database file [env: RANGER_DB]
  -h, --help          Print help
";

pub enum CommentCommands {
    Add { task: String, body: String },
    List { task: String },
}

pub fn parse(
    parser: &mut lexopt::Parser,
    globals: &mut Globals,
) -> Result<CommentCommands, lexopt::Error> {
    let sub = cli::subcommand(parser, globals, HELP)?;
    Ok(match sub.as_str() {
        "add" | "a" => {
            let mut task = None;
            let mut body = None;
            while let Some(arg) = parser.next()? {
                match arg {
                    Value(v) if task.is_none() => task = Some(v.string()?),
                    Value(v) if body.is_none() => body = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            CommentCommands::Add {
                task: cli::required(task, "<TASK>")?,
                body: cli::required(body, "<BODY>")?,
            }
        }
        "list" | "ls" => {
            let mut task = None;
            while let Some(arg) = parser.next()? {
                match arg {
                    Value(v) if task.is_none() => task = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            CommentCommands::List {
                task: cli::required(task, "<TASK>")?,
            }
        }
        _ => return Err(cli::error(format!("unrecognized comment command '{sub}'"))),
    })
}

pub async fn run(
    pool: &SqlitePool,
    command: CommentCommands,
    json: bool,
) -> Result<(), RangerError> {
    let backlog_scope = super::task::default_backlog_id(pool).await;
    let mut conn = pool.acquire().await?;

    match command {
        CommentCommands::Add { task, body } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            let comment = ops::comment::add(&mut conn, t.id, &body).await?;
            output::print(&comment, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
        CommentCommands::List { task } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            let comments = ops::comment::list(&mut conn, t.id).await?;
            output::print_list(&comments, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
    }
    Ok(())
}
