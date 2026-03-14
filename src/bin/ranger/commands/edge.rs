use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::models::EdgeType;
use ranger::ops;

use super::task::default_backlog_id;
use crate::completions;
use crate::output;

#[derive(Subcommand)]
pub enum EdgeCommands {
    /// Add an edge between tasks
    Add {
        /// Source task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        from: String,
        /// Edge type: blocks or before
        edge_type: EdgeType,
        /// Target task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        to: String,
    },
    /// Remove an edge between tasks
    #[command(visible_alias = "rm")]
    Remove {
        /// Source task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        from: String,
        /// Edge type: blocks or before
        edge_type: EdgeType,
        /// Target task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        to: String,
    },
    /// List edges for a task
    #[command(visible_alias = "ls")]
    List {
        /// Task key or prefix (omit to list all edges)
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        task: Option<String>,
    },
}

pub async fn run(pool: &SqlitePool, command: EdgeCommands, json: bool) -> Result<()> {
    let backlog_scope = default_backlog_id(pool).await;
    let mut conn = pool.acquire().await?;

    match command {
        EdgeCommands::Add {
            from,
            edge_type,
            to,
        } => {
            let from_task = ops::task::get_by_key_prefix(&mut conn, &from, backlog_scope).await?;
            let to_task = ops::task::get_by_key_prefix(&mut conn, &to, backlog_scope).await?;

            let edge = ops::edge::add(&mut conn, from_task.id, to_task.id, edge_type).await?;
            output::print(&edge, json, |e| {
                println!("{} {} {}", from, e.edge_type, to);
            });
        }
        EdgeCommands::Remove {
            from,
            edge_type,
            to,
        } => {
            let from_task = ops::task::get_by_key_prefix(&mut conn, &from, backlog_scope).await?;
            let to_task = ops::task::get_by_key_prefix(&mut conn, &to, backlog_scope).await?;

            let removed = ops::edge::remove(&mut conn, from_task.id, to_task.id, edge_type).await?;
            if !json {
                if removed {
                    println!("Removed edge from {} to {}", from, to);
                } else {
                    println!("No matching edge found");
                }
            }
        }
        EdgeCommands::List { task } => {
            let edges = if let Some(ref key) = task {
                let t = ops::task::get_by_key_prefix(&mut conn, key, backlog_scope).await?;
                ops::edge::list_for_task(&mut conn, t.id).await?
            } else {
                ops::edge::list_all(&mut conn).await?
            };

            if json {
                output::print_list(&edges, json, |_| {});
            } else {
                for e in &edges {
                    let from = ops::task::get_by_id(&mut conn, e.from_task_id).await?;
                    let to = ops::task::get_by_id(&mut conn, e.to_task_id).await?;
                    println!(
                        "{} {} {}",
                        &from.key[..8.min(from.key.len())],
                        e.edge_type,
                        &to.key[..8.min(to.key.len())],
                    );
                }
            }
        }
    }
    Ok(())
}
