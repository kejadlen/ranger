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
    List,
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
        TagCommands::List => {
            let tags = ops::tag::list_all(&mut conn).await?;
            output::print_list(&tags, json, |t| println!("{}", t.name));
        }
    }
    Ok(())
}
