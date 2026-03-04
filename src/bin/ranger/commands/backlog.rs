use clap::Subcommand;
use ranger::db::SqlitePool;
use ranger::models::Backlog;
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum BacklogCommands {
    /// Create a new backlog
    Create {
        /// Name for the backlog
        name: String,
    },
    /// List all backlogs
    List,
    /// Show a backlog's details
    Show {
        /// Key or key prefix of the backlog
        key: String,
    },
}

pub async fn run(pool: &SqlitePool, command: BacklogCommands, json: bool) -> anyhow::Result<()> {
    match command {
        BacklogCommands::Create { name } => {
            let backlog = ops::backlog::create(pool, &name).await?;
            output::print(&backlog, json, print_backlog);
        }
        BacklogCommands::List => {
            let backlogs = ops::backlog::list(pool).await?;
            output::print_list(&backlogs, json, print_backlog);
        }
        BacklogCommands::Show { key } => {
            let backlog = ops::backlog::get_by_key_prefix(pool, &key).await?;
            output::print(&backlog, json, print_backlog_detail);
        }
    }
    Ok(())
}

fn print_backlog(b: &Backlog) {
    println!("{} {}", &b.key[..8], b.name);
}

fn print_backlog_detail(b: &Backlog) {
    println!("Key:     {}", b.key);
    println!("Name:    {}", b.name);
    println!("Created: {}", b.created_at);
    println!("Updated: {}", b.updated_at);
}
