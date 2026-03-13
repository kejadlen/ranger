use std::collections::HashMap;

use clap::{Args, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use color_eyre::eyre::{Result, bail};
use ranger::db::{SqliteConnection, SqlitePool};
use ranger::key;
use ranger::models::{State, Task};
use ranger::ops;
use ranger::ops::task::{ListFilter, Placement};

use crate::completions;
use crate::output;

/// Positioning flags shared by create, edit, and move.
#[derive(Args)]
pub struct PositionArgs {
    /// Place before this task key
    #[arg(long, short = 'B', add = ArgValueCompleter::new(completions::complete_task_keys))]
    before: Option<String>,
    /// Place after this task key
    #[arg(long, short = 'A', add = ArgValueCompleter::new(completions::complete_task_keys))]
    after: Option<String>,
}

impl PositionArgs {
    async fn resolve(
        self,
        conn: &mut SqliteConnection,
        backlog_id: Option<i64>,
    ) -> Result<Option<PositionAnchors>> {
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

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    #[command(visible_alias = "new")]
    Create {
        /// Task title
        title: String,
        /// Backlog name
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG", add = ArgValueCompleter::new(completions::complete_backlog_names))]
        backlog: String,
        /// Task description
        #[arg(long)]
        description: Option<String>,
        /// Initial state (icebox, queued, in_progress, done)
        #[arg(long)]
        state: Option<String>,
        /// Parent task key or prefix (makes this a subtask)
        #[arg(long, add = ArgValueCompleter::new(completions::complete_task_keys))]
        parent: Option<String>,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// List tasks
    #[command(visible_alias = "ls")]
    List {
        /// Filter by backlog name
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG", add = ArgValueCompleter::new(completions::complete_backlog_names))]
        backlog: Option<String>,
        /// Filter by state
        #[arg(long)]
        state: Option<String>,
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        /// Include archived tasks
        #[arg(long)]
        archived: bool,
    },
    /// Show task details
    #[command(visible_alias = "s")]
    Show {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },
    /// Edit a task
    #[command(visible_alias = "e")]
    Edit {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// New state
        #[arg(long)]
        state: Option<String>,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// Move a task's position within its backlog
    #[command(visible_alias = "mv")]
    Move {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
        #[command(flatten)]
        position: PositionArgs,
    },

    /// Delete a task entirely
    #[command(visible_alias = "del")]
    Delete {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },

    /// Archive a task
    Archive {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },

    /// Unarchive a task
    Unarchive {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },
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

pub async fn run(pool: &SqlitePool, command: TaskCommands, json: bool) -> Result<()> {
    let backlog_scope = default_backlog_id(pool).await;

    match command {
        TaskCommands::Create {
            title,
            backlog,
            description,
            state,
            parent,
            position,
        } => {
            let mut tx = pool.begin().await?;

            let bl = ops::backlog::get_by_name(&mut tx, &backlog).await?;
            let parent_id = if let Some(parent_key) = &parent {
                Some(
                    ops::task::get_by_key_prefix(&mut tx, parent_key, Some(bl.id))
                        .await?
                        .id,
                )
            } else {
                None
            };
            let anchors = position.resolve(&mut tx, Some(bl.id)).await?;
            let state = state.map(|s| s.parse::<State>()).transpose()?;

            let task = ops::task::create(
                &mut tx,
                ops::task::CreateTask {
                    title: &title,
                    backlog_id: bl.id,
                    state,
                    parent_id,
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
                state: state.map(|s| s.parse::<State>()).transpose()?,
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
            let state = state.map(|s| s.parse::<State>()).transpose()?;
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
                    println!(
                        "Moved {} {}",
                        output::format_key_from_map(&task.key, &prefixes),
                        task.title
                    );
                }
                None => bail!("--before or --after is required"),
            }
        }
        TaskCommands::Delete { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, backlog_scope).await?;
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            ops::task::delete(&mut conn, task.id).await?;
            println!(
                "Deleted {} {}",
                output::format_key_from_map(&task.key, &prefixes),
                task.title
            );
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
    if let Some(pid) = t.parent_id {
        println!("Parent:  {}", pid);
    }
    println!("Created: {}", t.created_at);
    println!("Updated: {}", t.updated_at);
}
