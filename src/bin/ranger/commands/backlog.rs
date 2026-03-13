use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::key;
use ranger::models::{Backlog, State};
use ranger::ops;
use ranger::ops::task::ListFilter;

use crate::completions;
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
        #[arg(env = "RANGER_DEFAULT_BACKLOG", add = ArgValueCompleter::new(completions::complete_backlog_names))]
        name: String,

        /// Show only done tasks
        #[arg(long)]
        done: bool,
    },
    /// Rebalance task positions in a backlog
    Rebalance {
        /// Backlog name
        #[arg(env = "RANGER_DEFAULT_BACKLOG", add = ArgValueCompleter::new(completions::complete_backlog_names))]
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
        BacklogCommands::Show { name, done } => {
            let backlog = ops::backlog::get_by_name(&mut conn, &name).await?;

            let states: Vec<State> = if done {
                vec![State::Done]
            } else {
                vec![State::InProgress, State::Queued, State::Icebox]
            };

            if json {
                let mut state_groups = serde_json::Map::new();
                for state in &states {
                    let filter = ListFilter {
                        state: Some(state.clone()),
                        ..Default::default()
                    };
                    let tasks = ops::task::list(&mut conn, backlog.id, &filter).await?;
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
                let backlog_keys = ops::task::keys_for_backlog(&mut conn, backlog.id).await?;
                let prefixes = key::unique_prefix_lengths(&backlog_keys);

                print_backlog_detail(&backlog);

                for state in &states {
                    let filter = ListFilter {
                        state: Some(state.clone()),
                        ..Default::default()
                    };
                    let tasks = ops::task::list(&mut conn, backlog.id, &filter).await?;
                    if !tasks.is_empty() {
                        println!("\n[{}]", state);
                        for t in &tasks {
                            let tags = ops::tag::list_for_task(&mut conn, t.id).await?;
                            let tag_str = if tags.is_empty() {
                                String::new()
                            } else {
                                let names: Vec<String> = tags
                                    .iter()
                                    .map(|tg| format!("\x1b[36m#{}\x1b[0m", tg.name))
                                    .collect();
                                format!(" {}", names.join(" "))
                            };
                            println!(
                                "  {} {}{}",
                                output::format_key_from_map(&t.key, &prefixes),
                                t.title,
                                tag_str
                            );
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
