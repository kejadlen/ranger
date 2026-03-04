use clap::Subcommand;
use ranger_lib::db::SqlitePool;
use ranger_lib::ops;

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

pub async fn run(pool: &SqlitePool, command: BlockerCommands, json: bool) -> anyhow::Result<()> {
    match command {
        BlockerCommands::Add { task, blocked_by } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let bt = ops::task::get_by_key_prefix(pool, &blocked_by).await?;
            let blocker = ops::blocker::add(pool, t.id, bt.id).await?;
            output::print(&blocker, json, |_| {
                println!("{} blocked by {} {}", &t.key[..8], &bt.key[..8], bt.title);
            });
        }
        BlockerCommands::Remove { task, blocked_by } => {
            let t = ops::task::get_by_key_prefix(pool, &task).await?;
            let bt = ops::task::get_by_key_prefix(pool, &blocked_by).await?;
            ops::blocker::remove(pool, t.id, bt.id).await?;
            println!("Removed blocker {} from {}", &bt.key[..8], &t.key[..8]);
        }
    }
    Ok(())
}
