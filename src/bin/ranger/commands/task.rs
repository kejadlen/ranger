use std::collections::HashMap;

use lexopt::prelude::*;
use ranger::db::{SqliteConnection, SqlitePool};
use ranger::error::RangerError as Error;
use ranger::key;
use ranger::models::{State, Task};
use ranger::ops;
use ranger::ops::task::{ListFilter, Placement};

use crate::cli::{self, Globals, OwnedArg};
use crate::output;

const HELP: &str = "\
Manage tasks

Usage: ranger task [OPTIONS] <COMMAND>

Commands:
  create <TITLE>   Create a new task [alias: new]
      [--backlog <NAME>] [--description <TEXT>] [--state <STATE>]
      [-B|--before <KEY>] [-A|--after <KEY>]
  list             List tasks [alias: ls]
      [--backlog <NAME>] [--state <STATE>] [--tag <TAG>] [--archived]
  show <KEY>       Show task details [alias: s]
  edit <KEY>       Edit a task [alias: e]
      [--title <TITLE>] [--description <TEXT>] [--state <STATE>]
      [-B|--before <KEY>] [-A|--after <KEY>]
  move <KEY>       Move a task's position within its backlog [alias: mv]
      (-B|--before <KEY> | -A|--after <KEY>)
  delete <KEY> [-y|--yes]  Delete a task entirely [alias: del]
  archive <KEY>    Archive a task
  unarchive <KEY>  Unarchive a task

KEY may be any unique key prefix. STATE is one of icebox, ready,
in_progress, done. --backlog defaults to $RANGER_DEFAULT_BACKLOG.

Options:
      --json          Output as JSON
      --color <WHEN>  When to colorize output [auto|always|never]
      --db <PATH>     Path to database file [env: RANGER_DB]
  -h, --help          Print help
";

/// Positioning flags shared by create, edit, and move.
#[derive(Default)]
pub struct PositionArgs {
    /// Place before this task key
    before: Option<String>,
    /// Place after this task key
    after: Option<String>,
}

impl PositionArgs {
    async fn resolve(
        self,
        conn: &mut SqliteConnection,
        backlog_id: Option<i64>,
    ) -> Result<Option<PositionAnchors>, Error> {
        match (self.before, self.after) {
            (None, None) => Ok(None),
            (Some(b), None) => {
                let before = ops::task::get_by_key_prefix(conn, &b, backlog_id).await?;
                Ok(Some(PositionAnchors::Before(before)))
            }
            (None, Some(a)) => {
                let after = ops::task::get_by_key_prefix(conn, &a, backlog_id).await?;
                Ok(Some(PositionAnchors::After(after)))
            }
            (Some(b), Some(a)) => {
                let before = ops::task::get_by_key_prefix(conn, &b, backlog_id).await?;
                let after = ops::task::get_by_key_prefix(conn, &a, backlog_id).await?;
                Ok(Some(PositionAnchors::Between { before, after }))
            }
        }
    }
}

enum PositionAnchors {
    Before(Task),
    After(Task),
    Between { before: Task, after: Task },
}

impl PositionAnchors {
    fn as_placement(&self) -> Placement<'_> {
        match self {
            PositionAnchors::Before(t) => Placement::Before(t),
            PositionAnchors::After(t) => Placement::After(t),
            PositionAnchors::Between { before, after } => Placement::Between { before, after },
        }
    }
}

pub enum TaskCommands {
    Create {
        title: String,
        backlog: String,
        description: Option<String>,
        state: Option<State>,
        position: PositionArgs,
    },
    List {
        backlog: Option<String>,
        state: Option<State>,
        tag: Option<String>,
        archived: bool,
    },
    Show {
        key: String,
    },
    Edit {
        key: String,
        title: Option<String>,
        description: Option<String>,
        state: Option<State>,
        position: PositionArgs,
    },
    Move {
        key: String,
        position: PositionArgs,
    },
    Delete {
        key: String,
        yes: bool,
    },
    Archive {
        key: String,
    },
    Unarchive {
        key: String,
    },
}

pub fn parse(
    parser: &mut lexopt::Parser,
    globals: &mut Globals,
) -> Result<TaskCommands, lexopt::Error> {
    let sub = cli::subcommand(parser, globals, HELP)?;
    Ok(match sub.as_str() {
        "create" | "new" => {
            let mut title = None;
            let mut backlog = None;
            let mut description = None;
            let mut state = None;
            let mut position = PositionArgs::default();
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("backlog") => backlog = Some(parser.value()?.string()?),
                    Long("description") => description = Some(parser.value()?.string()?),
                    Long("state") => state = Some(parser.value()?.parse()?),
                    Short('B') | Long("before") => {
                        position.before = Some(parser.value()?.string()?)
                    }
                    Short('A') | Long("after") => position.after = Some(parser.value()?.string()?),
                    Value(v) if title.is_none() => title = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TaskCommands::Create {
                title: cli::required(title, "<TITLE>")?,
                backlog: cli::required(backlog.or_else(cli::default_backlog), "--backlog")?,
                description,
                state,
                position,
            }
        }
        "list" | "ls" => {
            let mut backlog = None;
            let mut state = None;
            let mut tag = None;
            let mut archived = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("backlog") => backlog = Some(parser.value()?.string()?),
                    Long("state") => state = Some(parser.value()?.parse()?),
                    Long("tag") => tag = Some(parser.value()?.string()?),
                    Long("archived") => archived = true,
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TaskCommands::List {
                backlog: backlog.or_else(cli::default_backlog),
                state,
                tag,
                archived,
            }
        }
        "show" | "s" => TaskCommands::Show {
            key: parse_key(parser, globals)?,
        },
        "edit" | "e" => {
            let mut key = None;
            let mut title = None;
            let mut description = None;
            let mut state = None;
            let mut position = PositionArgs::default();
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("title") => title = Some(parser.value()?.string()?),
                    Long("description") => description = Some(parser.value()?.string()?),
                    Long("state") => state = Some(parser.value()?.parse()?),
                    Short('B') | Long("before") => {
                        position.before = Some(parser.value()?.string()?)
                    }
                    Short('A') | Long("after") => position.after = Some(parser.value()?.string()?),
                    Value(v) if key.is_none() => key = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TaskCommands::Edit {
                key: cli::required(key, "<KEY>")?,
                title,
                description,
                state,
                position,
            }
        }
        "move" | "mv" => {
            let mut key = None;
            let mut position = PositionArgs::default();
            while let Some(arg) = parser.next()? {
                match arg {
                    Short('B') | Long("before") => {
                        position.before = Some(parser.value()?.string()?)
                    }
                    Short('A') | Long("after") => position.after = Some(parser.value()?.string()?),
                    Value(v) if key.is_none() => key = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TaskCommands::Move {
                key: cli::required(key, "<KEY>")?,
                position,
            }
        }
        "delete" | "del" => {
            let mut key = None;
            let mut yes = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Short('y') | Long("yes") => yes = true,
                    Value(v) if key.is_none() => key = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            TaskCommands::Delete {
                key: cli::required(key, "<KEY>")?,
                yes,
            }
        }
        "archive" => TaskCommands::Archive {
            key: parse_key(parser, globals)?,
        },
        "unarchive" => TaskCommands::Unarchive {
            key: parse_key(parser, globals)?,
        },
        _ => return Err(cli::error(format!("unrecognized task command '{sub}'"))),
    })
}

/// Parse a subcommand whose only argument is a task key.
fn parse_key(parser: &mut lexopt::Parser, globals: &mut Globals) -> Result<String, lexopt::Error> {
    let mut key = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Value(v) if key.is_none() => key = Some(v.string()?),
            other => {
                let other = OwnedArg::from(other);
                globals.consume(other, parser, HELP)?;
            }
        }
    }
    cli::required(key, "<KEY>")
}

/// Resolve `RANGER_DEFAULT_BACKLOG` to a backlog ID, if set.
/// Returns `None` when the env var is absent or the backlog doesn't exist.
pub async fn default_backlog_id(pool: &SqlitePool) -> Option<i64> {
    let name = std::env::var("RANGER_DEFAULT_BACKLOG").ok()?;
    let mut conn = pool.acquire().await.ok()?;
    ops::backlog::get_by_name(&mut conn, &name)
        .await
        .ok()
        .map(|b| b.id)
}

pub async fn run(pool: &SqlitePool, command: TaskCommands, json: bool) -> Result<(), Error> {
    let backlog_scope = default_backlog_id(pool).await;

    match command {
        TaskCommands::Create {
            title,
            backlog,
            description,
            state,
            position,
        } => {
            let mut tx = pool.begin().await?;

            let bl = ops::backlog::get_by_name(&mut tx, &backlog).await?;
            let anchors = position.resolve(&mut tx, Some(bl.id)).await?;

            let task = ops::task::create(
                &mut tx,
                ops::task::CreateTask {
                    title: &title,
                    backlog_id: bl.id,
                    state,
                    description: description.as_deref(),
                },
            )
            .await?;

            if let Some(ref anchors) = anchors {
                ops::task::move_task(&mut tx, &task, anchors.as_placement()).await?;
            }

            tx.commit().await?;

            let mut conn = pool.acquire().await?;
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            output::print(&task, json, |t| print_task(t, &prefixes));
        }
        TaskCommands::List {
            backlog,
            state,
            tag,
            archived,
        } => {
            let mut conn = pool.acquire().await?;
            let filter = ListFilter {
                state,
                include_archived: archived,
                tag,
            };

            if let Some(backlog_name) = &backlog {
                let bl = ops::backlog::get_by_name(&mut conn, backlog_name).await?;
                let backlog_keys = ops::task::keys_for_backlog(&mut conn, bl.id).await?;
                let prefixes = key::unique_prefix_lengths(&backlog_keys);
                let tasks = ops::task::list(&mut conn, bl.id, &filter).await?;
                output::print_list(&tasks, json, |t| print_task(t, &prefixes));
            } else {
                // List all tasks (no backlog filter)
                let all_keys = ops::task::all_keys(&mut conn).await?;
                let prefixes = key::unique_prefix_lengths(&all_keys);
                let backlogs = ops::backlog::list(&mut conn).await?;
                let mut all_tasks = Vec::new();
                for bl in &backlogs {
                    let tasks = ops::task::list(&mut conn, bl.id, &filter).await?;
                    for t in tasks {
                        if !all_tasks.iter().any(|at: &Task| at.id == t.id) {
                            all_tasks.push(t);
                        }
                    }
                }
                output::print_list(&all_tasks, json, |t| print_task(t, &prefixes));
            }
        }
        TaskCommands::Show { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let comments = ops::comment::list(&mut conn, task.id).await?;
            let tags = ops::tag::list_for_task(&mut conn, task.id).await?;

            if json {
                let detail = serde_json::json!({
                    "task": task,
                    "comments": comments,
                    "tags": tags,
                });
                println!("{}", serde_json::to_string_pretty(&detail).unwrap());
            } else {
                let all_keys = ops::task::all_keys(&mut conn).await?;
                let prefixes = key::unique_prefix_lengths(&all_keys);

                print_task_detail(&task, &prefixes);
                if !tags.is_empty() {
                    let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
                    println!("Tags:    {}", tag_names.join(", "));
                }
                if !comments.is_empty() {
                    println!();
                    for c in &comments {
                        println!("--- {} ---", c.created_at);
                        println!("{}", c.body);
                    }
                }
            }
        }
        TaskCommands::Edit {
            key,
            title,
            description,
            state,
            position,
        } => {
            let mut conn = pool.acquire().await?;
            let anchors = position.resolve(&mut conn, backlog_scope).await?;

            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let updated = ops::task::edit(
                &mut conn,
                task.id,
                title.as_deref(),
                description.as_deref(),
                state,
            )
            .await?;

            if let Some(ref anchors) = anchors {
                ops::task::move_task(&mut conn, &updated, anchors.as_placement()).await?;
            }

            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            output::print(&updated, json, |t| print_task(t, &prefixes));
        }
        TaskCommands::Move { key, position } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let anchors = position.resolve(&mut conn, backlog_scope).await?;

            match anchors {
                Some(anchors) => {
                    ops::task::move_task(&mut conn, &task, anchors.as_placement()).await?;
                    let all_keys = ops::task::all_keys(&mut conn).await?;
                    let prefixes = key::unique_prefix_lengths(&all_keys);
                    output::print(&task, json, |t| {
                        println!(
                            "Moved {} {}",
                            output::format_key_from_map(&t.key, &prefixes),
                            t.title
                        );
                    });
                }
                None => {
                    return Err(Error::Usage("--before or --after is required".into()));
                }
            }
        }
        TaskCommands::Delete { key, yes } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;

            if !yes {
                let prompt = format!("Delete task '{}'?", task.title);
                match output::confirm(&prompt) {
                    output::Confirm::Yes => {}
                    output::Confirm::No => {
                        println!("Aborted.");
                        return Ok(());
                    }
                    output::Confirm::NeedsFlag => {
                        return Err(Error::Usage(format!(
                            "refusing to delete task '{}' without confirmation; pass --yes to proceed",
                            task.title
                        )));
                    }
                }
            }

            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            ops::task::delete(&mut conn, task.id).await?;
            output::print(&task, json, |t| {
                println!(
                    "Deleted {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
        TaskCommands::Archive { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let updated = ops::task::set_archived(&mut conn, task.id, true).await?;
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            output::print(&updated, json, |t| {
                println!(
                    "Archived {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
        TaskCommands::Unarchive { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let updated = ops::task::set_archived(&mut conn, task.id, false).await?;
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            output::print(&updated, json, |t| {
                println!(
                    "Unarchived {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
    }
    Ok(())
}

fn print_task(t: &Task, prefixes: &HashMap<String, usize>) {
    println!(
        "{} [{}] {}",
        output::format_key_from_map(&t.key, prefixes),
        t.state,
        t.title
    );
}

fn print_task_detail(t: &Task, prefixes: &HashMap<String, usize>) {
    println!("Key:     {}", output::format_key_from_map(&t.key, prefixes));
    println!("Title:   {}", t.title);
    println!("State:   {}", t.state);
    if t.archived {
        println!("Archived: yes");
    }
    if let Some(desc) = &t.description {
        println!("Desc:    {}", desc);
    }
    println!("Created: {}", t.created_at);
    println!("Updated: {}", t.updated_at);
    if let Some(done_at) = &t.done_at {
        println!("Done:    {}", done_at);
    }
}
