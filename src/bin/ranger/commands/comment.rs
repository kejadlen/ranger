use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;
use ranger::db::SqlitePool;
use ranger::error::RangerError;
use ranger::ops;

use crate::completions;
use crate::output;

#[derive(Subcommand)]
pub enum CommentCommands {
    /// Add a comment to a task
    #[command(visible_alias = "a")]
    Add {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        task: String,
        /// Comment body
        body: String,
    },
    /// List comments on a task
    #[command(visible_alias = "ls")]
    List {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        task: String,
    },
}

pub async fn run(
    pool: &SqlitePool,
    command: CommentCommands,
    json: bool,
) -> Result<(), RangerError> {
    let backlog_scope = super::task::default_backlog_id(pool).await;
    let mut conn = pool.acquire().await?;

    match command {
        CommentCommands::Add { task, body } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            let comment = ops::comment::add(&mut conn, t.id, &body).await?;
            output::print(&comment, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
        CommentCommands::List { task } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, backlog_scope).await?;
            let comments = ops::comment::list(&mut conn, t.id).await?;
            output::print_list(&comments, json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
    }
    Ok(())
}
