use clap::Subcommand;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::models::{Backlog, State};
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum BacklogCommands {
    /// Create a new backlog
    #[command(visible_alias = "new")]
    Create {
        /// Name for the backlog
        name: String,
    },
    /// List all backlogs
    #[command(visible_alias = "ls")]
    List,
    /// Show a backlog's details
    #[command(visible_alias = "s")]
    Show {
        /// Backlog name
        #[arg(env = "RANGER_DEFAULT_BACKLOG")]
        name: String,
    },
    /// Rebalance task positions in a backlog
    Rebalance {
        /// Backlog name
        #[arg(env = "RANGER_DEFAULT_BACKLOG")]
        name: String,
    },
}

pub async fn run(pool: &SqlitePool, command: BacklogCommands, json: bool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    match command {
        BacklogCommands::Create { name } => {
            let backlog = ops::backlog::create(&mut conn, &name).await?;
            output::print(&backlog, json, print_backlog);
        }
        BacklogCommands::List => {
            let backlogs = ops::backlog::list(&mut conn).await?;
            output::print_list(&backlogs, json, print_backlog);
        }
        BacklogCommands::Rebalance { name } => {
            let backlog = ops::backlog::get_by_name(&mut conn, &name).await?;
            let count = ops::task::rebalance(&mut conn, backlog.id).await?;
            println!("Rebalanced {count} tasks in {name}");
        }
        BacklogCommands::Show { name } => {
            let backlog = ops::backlog::get_by_name(&mut conn, &name).await?;

            if json {
                let mut state_groups = serde_json::Map::new();
                for state in [State::Done, State::InProgress, State::Queued, State::Icebox] {
                    let tasks = ops::task::list(&mut conn, backlog.id, Some(state.clone())).await?;
                    if !tasks.is_empty() {
                        state_groups
                            .insert(state.to_string(), serde_json::to_value(&tasks).unwrap());
                    }
                }
                let detail = serde_json::json!({
                    "backlog": backlog,
                    "tasks": state_groups,
                });
                println!("{}", serde_json::to_string_pretty(&detail).unwrap());
            } else {
                print_backlog_detail(&backlog);

                for state in [State::Done, State::InProgress, State::Queued, State::Icebox] {
                    let tasks = ops::task::list(&mut conn, backlog.id, Some(state.clone())).await?;
                    if !tasks.is_empty() {
                        println!("\n[{}]", state);
                        for t in &tasks {
                            println!("  {} {}", &t.key[..8], t.title);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_backlog(b: &Backlog) {
    println!("{}", b.name);
}

fn print_backlog_detail(b: &Backlog) {
    println!("Name:    {}", b.name);
    println!("Created: {}", b.created_at);
    println!("Updated: {}", b.updated_at);
}
