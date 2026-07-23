use lexopt::prelude::*;
use ranger::db::SqlitePool;
use ranger::error::RangerError;
use ranger::key;
use ranger::models::{Backlog, State};
use ranger::ops;
use ranger::ops::task::ListFilter;

use crate::cli::{self, Globals, OwnedArg};
use crate::output;

const HELP: &str = "\
Manage backlogs

Usage: ranger backlog [OPTIONS] <COMMAND>

Commands:
  create <NAME>             Create a new backlog [alias: new]
  list                      List all backlogs [alias: ls]
  show [NAME] [--done]      Show a backlog's details [alias: s]
  delete <NAME> [-y|--yes]  Delete a backlog and all its tasks [alias: rm]
  rebalance [NAME]          Rebalance task positions in a backlog

NAME defaults to $RANGER_DEFAULT_BACKLOG for show and rebalance.

Options:
      --done          Show only done tasks (show)
  -y, --yes           Skip the confirmation prompt (delete)
      --json          Output as JSON
      --color <WHEN>  When to colorize output [auto|always|never]
      --db <PATH>     Path to database file [env: RANGER_DB]
  -h, --help          Print help
";

pub enum BacklogCommands {
    Create { name: String },
    List,
    Show { name: String, done: bool },
    Delete { name: String, yes: bool },
    Rebalance { name: String },
}

pub fn parse(
    parser: &mut lexopt::Parser,
    globals: &mut Globals,
) -> Result<BacklogCommands, lexopt::Error> {
    let sub = cli::subcommand(parser, globals, HELP)?;
    Ok(match sub.as_str() {
        "create" | "new" => {
            let mut name = None;
            while let Some(arg) = parser.next()? {
                match arg {
                    Value(v) if name.is_none() => name = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            BacklogCommands::Create {
                name: cli::required(name, "<NAME>")?,
            }
        }
        "list" | "ls" => {
            cli::drain(parser, globals, HELP)?;
            BacklogCommands::List
        }
        "show" | "s" => {
            let mut name = None;
            let mut done = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Long("done") => done = true,
                    Value(v) if name.is_none() => name = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            BacklogCommands::Show {
                name: cli::required(name.or_else(cli::default_backlog), "<NAME>")?,
                done,
            }
        }
        "delete" | "rm" => {
            let mut name = None;
            let mut yes = false;
            while let Some(arg) = parser.next()? {
                match arg {
                    Short('y') | Long("yes") => yes = true,
                    Value(v) if name.is_none() => name = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            BacklogCommands::Delete {
                name: cli::required(name, "<NAME>")?,
                yes,
            }
        }
        "rebalance" => {
            let mut name = None;
            while let Some(arg) = parser.next()? {
                match arg {
                    Value(v) if name.is_none() => name = Some(v.string()?),
                    other => {
                        let other = OwnedArg::from(other);
                        globals.consume(other, parser, HELP)?;
                    }
                }
            }
            BacklogCommands::Rebalance {
                name: cli::required(name.or_else(cli::default_backlog), "<NAME>")?,
            }
        }
        _ => return Err(cli::error(format!("unrecognized backlog command '{sub}'"))),
    })
}

pub async fn run(
    pool: &SqlitePool,
    command: BacklogCommands,
    json: bool,
) -> Result<(), RangerError> {
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
        BacklogCommands::Delete { name, yes } => {
            if !yes {
                let prompt = format!("Delete backlog '{name}' and all its tasks?");
                match output::confirm(&prompt) {
                    output::Confirm::Yes => {}
                    output::Confirm::No => {
                        println!("Aborted.");
                        return Ok(());
                    }
                    output::Confirm::NeedsFlag => {
                        return Err(RangerError::Usage(format!(
                            "refusing to delete backlog '{name}' without confirmation; pass --yes to proceed"
                        )));
                    }
                }
            }
            let backlog = ops::backlog::delete(&mut conn, &name).await?;
            output::print(&backlog, json, |b| println!("Deleted backlog: {}", b.name));
        }
        BacklogCommands::Rebalance { name } => {
            let backlog = ops::backlog::get_by_name(&mut conn, &name).await?;
            let count = ops::task::rebalance(&mut conn, backlog.id).await?;
            if json {
                let value = serde_json::json!({ "backlog": name, "rebalanced": count });
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            } else {
                println!("Rebalanced {count} tasks in {name}");
            }
        }
        BacklogCommands::Show { name, done } => {
            let backlog = ops::backlog::get_by_name(&mut conn, &name).await?;

            let states: Vec<State> = if done {
                vec![State::Done]
            } else {
                vec![State::InProgress, State::Ready, State::Icebox]
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
                                let names: Vec<String> =
                                    tags.iter().map(|tg| output::format_tag(&tg.name)).collect();
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
