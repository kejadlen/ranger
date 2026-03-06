use clap::Subcommand;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::key;
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum BlockerCommands {
    /// Add a blocker to a task
    #[command(visible_alias = "a")]
    Add {
        /// Task key or prefix (the blocked task)
        task: String,
        /// Blocking task key or prefix
        blocked_by: String,
    },
    /// Remove a blocker from a task
    #[command(visible_alias = "rm")]
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
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            let blocker = ops::blocker::add(&mut conn, t.id, bt.id).await?;
            output::print(&blocker, json, |_| {
                println!(
                    "{} blocked by {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    output::format_key_from_map(&bt.key, &prefixes),
                    bt.title
                );
            });
        }
        BlockerCommands::Remove { task, blocked_by } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task).await?;
            let bt = ops::task::get_by_key_prefix(&mut conn, &blocked_by).await?;
            let all_keys = ops::task::all_keys(&mut conn).await?;
            let prefixes = key::unique_prefix_lengths(&all_keys);
            ops::blocker::remove(&mut conn, t.id, bt.id).await?;
            println!(
                "Removed blocker {} from {}",
                output::format_key_from_map(&bt.key, &prefixes),
                output::format_key_from_map(&t.key, &prefixes)
            );
        }
    }
    Ok(())
}
