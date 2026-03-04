use clap::Subcommand;
use ranger::db::SqlitePool;
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum CommentCommands {
    /// Add a comment to a task
    Add {
        /// Task key or prefix
        task: String,
        /// Comment body
        body: String,
    },
    /// List comments on a task
    List {
        /// Task key or prefix
        task: String,
    },
}

pub async fn run(pool: &SqlitePool, command: CommentCommands, json: bool) -> anyhow::Result<()> {
    match command {
        CommentCommands::Add { task, body } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let comment = ops::comment::add(pool, t.id, &body).await?;
            output::print(&comment, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
        CommentCommands::List { task } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let comments = ops::comment::list(pool, t.id).await?;
            output::print_list(&comments, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
    }
    Ok(())
}
