use lexopt::prelude::*;
use ranger::db::SqlitePool;
use ranger::error::RangerError;
use ranger::ops;

use super::task::default_backlog_id;
use crate::cli::{self, Globals, OwnedArg};
use crate::output;

const HELP: &str = "\
Manage tags

Usage: ranger tag [OPTIONS] <COMMAND>

Commands:
  add <TASK> <TAG>     Add a tag to a task
  remove <TASK> <TAG>  Remove a tag from a task [alias: rm]
  list [--all]         List all tags [alias: ls]
  prune [--apply]      Remove unused tags (no associated tasks)

TASK may be any unique task key prefix.

Options:
      --all           Show tags from all backlogs (default: current backlog only)
      --apply         Actually delete the tags (default: dry run)
      --json          Output as JSON
      --color <WHEN>  When to colorize output [auto|always|never]
      --db <PATH>     Path to database file [env: RANGER_DB]
  -h, --help          Print help
";

pub enum TagCommands {
    Add { task: String, tag: String },
    Remove { task: String, tag: String },
    List { all: bool },
    Prune { apply: bool },
}

pub fn parse(
    parser: &mut lexopt::Parser,
    globals: &mut Globals,
) -> Result<TagCommands, lexopt::Error> {
    let sub = cli::subcommand(parser, globals, HELP)?;
    Ok(match sub.as_str() {
        "add" => {
            let (task, tag) = parse_task_tag(parser, globals)?;
            TagCommands::Add { task, tag }
        }
        "remove" | "rm" => {
            let (task, tag) = parse_task_tag(parser, globals)?;
            TagCommands::Remove { task, tag }
        }
        "list" | "ls" => {
            let mut all = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("all") => all = true,
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TagCommands::List { all }
        }
        "prune" => {
            let mut apply = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("apply") => apply = true,
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TagCommands::Prune { apply }
        }
        _ => return Err(cli::error(format!("unrecognized tag command '{sub}'"))),
    })
}

/// Parse the `<TASK> <TAG>` positional pair shared by add and remove.
fn parse_task_tag(
    parser: &mut lexopt::Parser,
    globals: &mut Globals,
) -> Result<(String, String), lexopt::Error> {
    let mut task = None;
    let mut tag = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Value(v) if task.is_none() => task = Some(v.string()?),
            Value(v) if tag.is_none() => tag = Some(v.string()?),
            other => {
                let other = OwnedArg::from(other);
                globals.consume(other, parser, HELP)?;
            }
        }
    }
    Ok((cli::required(task, "<TASK>")?, cli::required(tag, "<TAG>")?))
}

pub async fn run(pool: &SqlitePool, command: TagCommands, json: bool) -> Result<(), RangerError> {
    let backlog_scope = default_backlog_id(pool).await;
    let mut conn = pool.acquire().await?;

    match command {
        TagCommands::Add { task, tag } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            let created = ops::tag::add(&mut conn, t.id, &tag).await?;
            output::print(&created, json, |tg| {
                println!("Tagged {} with {}", task, tg.name)
            });
        }
        TagCommands::Remove { task, tag } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            ops::tag::remove(&mut conn, t.id, &tag).await?;
            if !json {
                println!("Removed tag {} from {}", tag, task);
            }
        }
        TagCommands::List { all } => {
            let tags = if all {
                ops::tag::list_all(&mut conn).await?
            } else if let Some(bl_id) = backlog_scope {
                ops::tag::list_for_backlog(&mut conn, bl_id).await?
            } else {
                ops::tag::list_all(&mut conn).await?
            };
            output::print_list(&tags, json, "No tags.", |t| println!("{}", t.name));
        }
        TagCommands::Prune { apply } => {
            let pruned = ops::tag::prune(&mut conn, !apply).await?;
            let label = if apply { "Removed" } else { "Would remove" };
            output::print_list(&pruned, json, "No unused tags to remove.", |t| {
                println!("{}: {}", label, t.name)
            });
        }
    }
    Ok(())
}
