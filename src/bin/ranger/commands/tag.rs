use clap::Subcommand;
use ranger::db::SqliteConnection;
use ranger::error::RangerError;
use ranger::ops;

use crate::app::Ranger;
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

/// The ID of the default backlog, whose tags `tag list` shows. `None` when
/// there's no default or the backlog doesn't exist.
async fn default_backlog_id(conn: &mut SqliteConnection, name: Option<&str>) -> Option<i64> {
    ops::backlog::get_by_name(conn, name?)
        .await
        .ok()
        .map(|b| b.id)
}

pub async fn run(ranger: &Ranger, command: TagCommands) -> Result<(), RangerError> {
    let mut conn = ranger.pool.acquire().await?;
    let scope = ranger.key_scope(&Default::default());

    match command {
        TagCommands::Add { task, tag } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, &scope).await?;
            let created = ops::tag::add(&mut conn, t.id, &tag).await?;
            output::print(&created, ranger.json, |tg| {
                println!("Tagged {} with {}", task, tg.name)
            });
        }
        TagCommands::Remove { task, tag } => {
            let t = ops::task::get_by_key_prefix(&mut conn, &task, &scope).await?;
            ops::tag::remove(&mut conn, t.id, &tag).await?;
            if !ranger.json {
                println!("Removed tag {} from {}", tag, task);
            }
        }
        TagCommands::List { all } => {
            let tags = if all {
                ops::tag::list_all(&mut conn).await?
            } else if let Some(bl_id) =
                default_backlog_id(&mut conn, ranger.default_backlog.as_deref()).await
            {
                ops::tag::list_for_backlog(&mut conn, bl_id).await?
            } else {
                ops::tag::list_all(&mut conn).await?
            };
            output::print_list(&tags, ranger.json, "No tags.", |t| println!("{}", t.name));
        }
        TagCommands::Prune { apply } => {
            let pruned = ops::tag::prune(&mut conn, !apply).await?;
            let label = if apply { "Removed" } else { "Would remove" };
            output::print_list(&pruned, ranger.json, "No unused tags to remove.", |t| {
                println!("{}: {}", label, t.name)
            });
        }
    }
    Ok(())
}
