use clap::Subcommand;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum BlockerCommands {
    /// Add a blocker to a task
    Add {
        /// Task key or prefix (the blocked task)
        task: String,
        /// Blocking task key or prefix
        blocked_by: String,
    },
    /// Remove a blocker from a task
    Remove {
        /// Task key or prefix (the blocked task)
        task: String,
        /// Blocking task key or prefix
        blocked_by: String,
    },
}

pub async fn run(pool: &SqlitePool, command: BlockerCommands, json: bool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    match command {
        BlockerCommands::Add { task, blocked_by } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task).await?;
            let bt = ops::task::get_by_key_prefix(&mut conn, &blocked_by).await?;
            let blocker = ops::blocker::add(&mut conn, t.id, bt.id).await?;
            output::print(&blocker, json, |_| {
                println!("{} blocked by {} {}", &t.key[..8], &bt.key[..8], bt.title);
            });
        }
        BlockerCommands::Remove { task, blocked_by } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task).await?;
            let bt = ops::task::get_by_key_prefix(&mut conn, &blocked_by).await?;
            ops::blocker::remove(&mut conn, t.id, bt.id).await?;
            println!("Removed blocker {} from {}", &bt.key[..8], &t.key[..8]);
        }
    }
    Ok(())
}
