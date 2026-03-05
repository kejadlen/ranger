use clap::{Args, Subcommand};
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::models::{State, Task};
use ranger::ops;

use crate::output;

/// Positioning flags shared by create and move.
#[derive(Args)]
pub struct PositionArgs {
    /// Place before this task key
    #[arg(long)]
    before: Option<String>,
    /// Place after this task key
    #[arg(long)]
    after: Option<String>,
}

impl PositionArgs {
    async fn resolve(self, pool: &SqlitePool) -> Result<(Option<i64>, Option<i64>)> {
        let before_id = if let Some(k) = &self.before {
            Some(ops::task::get_by_key_prefix(pool, k).await?.id)
        } else {
            None
        };
        let after_id = if let Some(k) = &self.after {
            Some(ops::task::get_by_key_prefix(pool, k).await?.id)
        } else {
            None
        };
        Ok((before_id, after_id))
    }
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    #[command(visible_alias = "new")]
    Create {
        /// Task title
        title: String,
        /// Backlog key or prefix
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: String,
        /// Task description
        #[arg(long)]
        description: Option<String>,
        /// Initial state (icebox, queued, in_progress, done)
        #[arg(long)]
        state: Option<String>,
        /// Parent task key or prefix (makes this a subtask)
        #[arg(long)]
        parent: Option<String>,
        /// Tags to add (comma-separated)
        #[arg(long)]
        tag: Option<String>,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// List tasks
    #[command(visible_alias = "ls")]
    List {
        /// Filter by backlog key or prefix
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: Option<String>,
        /// Filter by state
        #[arg(long)]
        state: Option<String>,
    },
    /// Show task details
    Show {
        /// Task key or prefix
        key: String,
    },
    /// Edit a task
    Edit {
        /// Task key or prefix
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
    },
    /// Move a task's position within a backlog
    #[command(visible_alias = "mv")]
    Move {
        /// Task key or prefix
        key: String,
        /// Backlog to reorder within
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: String,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// Add a task to a backlog
    Add {
        /// Task key or prefix
        task: String,
        /// Backlog key or prefix
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: String,
    },
    /// Remove a task from a backlog
    Remove {
        /// Task key or prefix
        task: String,
        /// Backlog key or prefix
        #[arg(long, env = "RANGER_DEFAULT_BACKLOG")]
        backlog: String,
    },
    /// Delete a task entirely
    #[command(visible_alias = "rm")]
    Delete {
        /// Task key or prefix
        key: String,
    },
}

pub async fn run(pool: &SqlitePool, command: TaskCommands, json: bool) -> Result<()> {
    match command {
        TaskCommands::Create {
            title,
            backlog,
            description,
            state,
            parent,
            tag,
            position,
        } => {
            let bl = ops::backlog::get_by_name(pool, &backlog).await?;
            let parent_id = if let Some(parent_key) = &parent {
                Some(ops::task::get_by_key_prefix(pool, parent_key).await?.id)
            } else {
                None
            };
            let (before_id, after_id) = position.resolve(pool).await?;
            let state = state.map(|s| s.parse::<State>()).transpose()?;

            let task = ops::task::create(
                pool,
                ops::task::CreateTask {
                    title: &title,
                    backlog_id: bl.id,
                    state,
                    parent_id,
                    description: description.as_deref(),
                    before_task_id: before_id,
                    after_task_id: after_id,
                },
            )
            .await?;

            if let Some(tags) = &tag {
                for tag_name in tags.split(',').map(str::trim) {
                    let t = ops::tag::get_or_create(pool, tag_name).await?;
                    ops::tag::add_to_task(pool, task.id, t.id).await?;
                }
            }

            output::print(&task, json, print_task);
        }
        TaskCommands::List { backlog, state } => {
            let state = state.map(|s| s.parse::<State>()).transpose()?;

            if let Some(backlog_key) = &backlog {
                let bl = ops::backlog::get_by_name(pool, backlog_key).await?;
                let tasks = ops::task::list(pool, bl.id, state).await?;
                output::print_list(&tasks, json, print_task);
            } else {
                // List all tasks (no backlog filter)
                let backlogs = ops::backlog::list(pool).await?;
                let mut all_tasks = Vec::new();
                for bl in &backlogs {
                    let tasks = ops::task::list(pool, bl.id, state.clone()).await?;
                    for t in tasks {
                        if !all_tasks.iter().any(|at: &Task| at.id == t.id) {
                            all_tasks.push(t);
                        }
                    }
                }
                output::print_list(&all_tasks, json, print_task);
            }
        }
        TaskCommands::Show { key } => {
            let task = ops::task::get_by_key_prefix(pool, &key).await?;
            let comments = ops::comment::list(pool, task.id).await?;
            let tags = ops::tag::list_for_task(pool, task.id).await?;
            let blockers = ops::blocker::list_for_task(pool, task.id).await?;

            if json {
                let detail = serde_json::json!({
                    "task": task,
                    "comments": comments,
                    "tags": tags,
                    "blockers": blockers,
                });
                println!("{}", serde_json::to_string_pretty(&detail).unwrap());
            } else {
                print_task_detail(&task);
                if !tags.is_empty() {
                    let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
                    println!("Tags:    {}", tag_names.join(", "));
                }
                if !blockers.is_empty() {
                    println!("Blocked by:");
                    for b in &blockers {
                        if let Ok(bt) = ops::task::get_by_id(pool, b.blocked_by_task_id).await {
                            println!("  {} {}", &bt.key[..8], bt.title);
                        }
                    }
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
        } => {
            let state = state.map(|s| s.parse::<State>()).transpose()?;

            let task = ops::task::get_by_key_prefix(pool, &key).await?;
            let updated = ops::task::edit(
                pool,
                task.id,
                title.as_deref(),
                description.as_deref(),
                state,
            )
            .await?;
            output::print(&updated, json, print_task);
        }
        TaskCommands::Move {
            key,
            backlog,
            position,
        } => {
            let bl = ops::backlog::get_by_name(pool, &backlog).await?;
            let task = ops::task::get_by_key_prefix(pool, &key).await?;
            let (before_id, after_id) = position.resolve(pool).await?;

            ops::task::move_task(pool, task.id, bl.id, before_id, after_id).await?;
            println!("Moved {} {}", &task.key[..8], task.title);
        }
        TaskCommands::Add { task, backlog } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let bl = ops::backlog::get_by_name(pool, &backlog).await?;
            ops::task::add_to_backlog(pool, t.id, bl.id).await?;
            println!("Added {} to {}", &t.key[..8], bl.name);
        }
        TaskCommands::Remove { task, backlog } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let bl = ops::backlog::get_by_name(pool, &backlog).await?;
            ops::task::remove_from_backlog(pool, t.id, bl.id).await?;
            println!("Removed {} from {}", &t.key[..8], bl.name);
        }
        TaskCommands::Delete { key } => {
            let task = ops::task::get_by_key_prefix(pool, &key).await?;
            ops::task::delete(pool, task.id).await?;
            println!("Deleted {} {}", &task.key[..8], task.title);
        }
    }
    Ok(())
}

fn print_task(t: &Task) {
    println!("{} [{}] {}", &t.key[..8], t.state, t.title);
}

fn print_task_detail(t: &Task) {
    println!("Key:     {}", t.key);
    println!("Title:   {}", t.title);
    println!("State:   {}", t.state);
    if let Some(desc) = &t.description {
        println!("Desc:    {}", desc);
    }
    if let Some(pid) = t.parent_id {
        println!("Parent:  {}", pid);
    }
    println!("Created: {}", t.created_at);
    println!("Updated: {}", t.updated_at);
}
