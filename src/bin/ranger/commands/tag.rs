use clap::Subcommand;
use ranger::db::SqlitePool;
use ranger::error::RangerError;
use ranger::ops;

use super::task::default_backlog_id;
use crate::output;

#[derive(Subcommand)]
pub enum TagCommands {
    /// Add a tag to a task
    Add {
        /// Task key or prefix
        task: String,
        /// Tag name
        tag: String,
    },
    /// Remove a tag from a task
    #[command(visible_alias = "rm")]
    Remove {
        /// Task key or prefix
        task: String,
        /// Tag name
        tag: String,
    },
    /// List all tags
    #[command(visible_alias = "ls")]
    List {
        /// Show tags from all backlogs (default: current backlog only)
        #[arg(long)]
        all: bool,
    },
    /// Remove unused tags (no associated tasks)
    Prune {
        /// Actually delete the tags (default: dry run)
        #[arg(long)]
        apply: bool,
    },
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
