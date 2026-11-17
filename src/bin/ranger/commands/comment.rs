use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;
use ranger::error::RangerError;
use ranger::ops;

use crate::app::Ranger;
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

pub async fn run(ranger: &Ranger, command: CommentCommands) -> Result<(), RangerError> {
    let mut conn = ranger.pool.acquire().await?;

    match command {
        CommentCommands::Add { task, body } => {
            let t = ops::task::get_by_key_prefix(
                &mut conn,
                &task,
                &ranger.key_scope(&Default::default()),
            )
            .await?;
            let comment = ops::comment::add(&mut conn, t.id, &body).await?;
            output::print(&comment, ranger.json, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
        CommentCommands::List { task } => {
            let t = ops::task::get_by_key_prefix(
                &mut conn,
                &task,
                &ranger.key_scope(&Default::default()),
            )
            .await?;
            let comments = ops::comment::list(&mut conn, t.id).await?;
            let note = format!("No comments on task {}.", t.key);
            output::print_list(&comments, ranger.json, &note, |c| {
                println!("[{}] {}", c.created_at, c.body);
            });
        }
    }
    Ok(())
}
