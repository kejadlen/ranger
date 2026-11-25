use std::collections::HashMap;
use std::io::Write;

use clap::{Args, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use ranger::db::SqliteConnection;
use ranger::error::RangerError as Error;
use ranger::filter::Filter;
use ranger::models::{State, Task};
use ranger::ops;
use ranger::ops::task::{ListFilter, Placement};

use crate::app::Ranger;
use crate::completions;
use crate::output;

/// Positioning flags shared by create, edit, and move.
#[derive(Args)]
pub struct PositionArgs {
    /// Place before this task key
    #[arg(long, short = 'B', add = ArgValueCompleter::new(completions::complete_task_keys))]
    before: Option<String>,
    /// Place after this task key
    #[arg(long, short = 'A', add = ArgValueCompleter::new(completions::complete_task_keys))]
    after: Option<String>,
}

impl PositionArgs {
    async fn resolve(
        self,
        conn: &mut SqliteConnection,
        scope: &Filter,
    ) -> Result<Option<PositionAnchors>, Error> {
        match (self.before, self.after) {
            (None, None) => Ok(None),
            (Some(b), None) => {
                let before = ops::task::get_by_key_prefix(conn, &b, scope).await?;
                Ok(Some(PositionAnchors::Before(before)))
            }
            (None, Some(a)) => {
                let after = ops::task::get_by_key_prefix(conn, &a, scope).await?;
                Ok(Some(PositionAnchors::After(after)))
            }
            (Some(b), Some(a)) => {
                let before = ops::task::get_by_key_prefix(conn, &b, scope).await?;
                let after = ops::task::get_by_key_prefix(conn, &a, scope).await?;
                Ok(Some(PositionAnchors::Between { before, after }))
            }
        }
    }
}

enum PositionAnchors {
    Before(Task),
    After(Task),
    Between { before: Task, after: Task },
}

impl PositionAnchors {
    fn as_placement(&self) -> Placement<'_> {
        match self {
            PositionAnchors::Before(t) => Placement::Before(t),
            PositionAnchors::After(t) => Placement::After(t),
            PositionAnchors::Between { before, after } => Placement::Between { before, after },
        }
    }
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    #[command(visible_alias = "new")]
    Create {
        /// Task title
        title: String,
        /// Task description
        #[arg(long)]
        description: Option<String>,
        /// Initial state
        #[arg(long)]
        state: Option<State>,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// List tasks
    #[command(visible_alias = "ls")]
    List {
        /// Filter by state
        #[arg(long)]
        state: Option<State>,
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        /// Include archived tasks
        #[arg(long)]
        archived: bool,
    },
    /// Show task details
    #[command(visible_alias = "s")]
    Show {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },
    /// Edit a task
    #[command(visible_alias = "e")]
    Edit {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// New state
        #[arg(long)]
        state: Option<State>,
        #[command(flatten)]
        position: PositionArgs,
    },
    /// Move a task's position within its backlog
    #[command(visible_alias = "mv")]
    Move {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
        #[command(flatten)]
        position: PositionArgs,
    },

    /// Delete a task entirely
    #[command(visible_alias = "del")]
    Delete {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Archive a task
    Archive {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },

    /// Unarchive a task
    Unarchive {
        /// Task key or prefix
        #[arg(add = ArgValueCompleter::new(completions::complete_task_keys))]
        key: String,
    },
}

pub async fn run(ranger: &Ranger, command: TaskCommands) -> Result<(), Error> {
    let pool = &ranger.pool;
    let json = ranger.json;
    let filter = &ranger.filter;
    let scope = filter;

    match command {
        TaskCommands::Create {
            title,
            description,
            state,
            position,
        } => {
            if state.is_some() && filter.has_state_term() {
                return Err(Error::StateConflict);
            }

            let mut tx = pool.begin().await?;

            let anchors = position.resolve(&mut tx, scope).await?;

            let task = ops::task::create(
                &mut tx,
                ops::task::CreateTask {
                    title: &title,
                    state: state.or_else(|| filter.state().cloned()),
                    description: description.as_deref(),
                },
            )
            .await?;

            for tag in filter.tags() {
                ops::tag::add(&mut tx, task.id, &tag.to_string()).await?;
            }

            if let Some(ref anchors) = anchors {
                ops::task::move_task(&mut tx, &task, anchors.as_placement()).await?;
            }

            tx.commit().await?;

            let mut conn = pool.acquire().await?;
            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            output::print(&task, json, |t| print_task(t, &prefixes));
        }
        TaskCommands::List {
            state,
            tag,
            archived,
        } => {
            let mut conn = pool.acquire().await?;
            let filter = ListFilter {
                state,
                include_archived: archived,
                tag,
                terms: filter.clone(),
            };

            let note = empty_task_note(&filter);
            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            let tasks = ops::task::list(&mut conn, &filter).await?;
            let tags = listed_tags(&mut conn, &tasks, &filter).await?;
            output::print_list_with(&tasks, json, &note, |ts| print_board(ts, &prefixes, &tags));
        }
        TaskCommands::Show { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;
            let comments = ops::comment::list(&mut conn, task.id).await?;
            let tags = ops::tag::list_for_task(&mut conn, task.id).await?;

            if json {
                let detail = serde_json::json!({
                    "task": task,
                    "comments": comments,
                    "tags": tags,
                });
                println!("{}", serde_json::to_string_pretty(&detail).unwrap());
            } else {
                let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;

                print_task_detail(&task, &prefixes);
                if !tags.is_empty() {
                    let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
                    println!("Tags:    {}", tag_names.join(", "));
                }
                if !comments.is_empty() {
                    println!();
                    for c in &comments {
                        println!("--- {} ---", c.created_at);
                        println!("{}", c.body);
                    }
                }
            }
        }
        TaskCommands::Edit {
            key,
            title,
            description,
            state,
            position,
        } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;

            let updated = if title.is_none()
                && description.is_none()
                && state.is_none()
                && position.before.is_none()
                && position.after.is_none()
            {
                edit_in_editor(&mut conn, ranger.editor.as_deref(), &task).await?
            } else {
                let anchors = position.resolve(&mut conn, scope).await?;
                let updated = ops::task::edit(
                    &mut conn,
                    task.id,
                    title.as_deref(),
                    description.as_deref(),
                    state,
                )
                .await?;

                if let Some(ref anchors) = anchors {
                    ops::task::move_task(&mut conn, &updated, anchors.as_placement()).await?;
                }
                updated
            };

            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            output::print(&updated, json, |t| print_task(t, &prefixes));
        }
        TaskCommands::Move { key, position } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;
            let anchors = position.resolve(&mut conn, scope).await?;

            match anchors {
                Some(anchors) => {
                    ops::task::move_task(&mut conn, &task, anchors.as_placement()).await?;
                    let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
                    output::print(&task, json, |t| {
                        println!(
                            "Moved {} {}",
                            output::format_key_from_map(&t.key, &prefixes),
                            t.title
                        );
                    });
                }
                None => {
                    return Err(Error::Usage("--before or --after is required".into()));
                }
            }
        }
        TaskCommands::Delete { key, yes } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;

            if !yes {
                let prompt = format!("Delete task '{}'?", task.title);
                match output::confirm(&prompt) {
                    output::Confirm::Yes => {}
                    output::Confirm::No => {
                        println!("Aborted.");
                        return Ok(());
                    }
                    output::Confirm::NeedsFlag => {
                        return Err(Error::Usage(format!(
                            "refusing to delete task '{}' without confirmation; pass --yes to proceed",
                            task.title
                        )));
                    }
                }
            }

            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            ops::task::delete(&mut conn, task.id).await?;
            output::print(&task, json, |t| {
                println!(
                    "Deleted {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
        TaskCommands::Archive { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;
            let updated = ops::task::set_archived(&mut conn, task.id, true).await?;
            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            output::print(&updated, json, |t| {
                println!(
                    "Archived {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
        TaskCommands::Unarchive { key } => {
            let mut conn = pool.acquire().await?;
            let task = ops::task::get_by_key_prefix(&mut conn, &key, scope).await?;
            let updated = ops::task::set_archived(&mut conn, task.id, false).await?;
            let prefixes = ops::task::prefix_lengths(&mut conn, scope).await?;
            output::print(&updated, json, |t| {
                println!(
                    "Unarchived {} {}",
                    output::format_key_from_map(&t.key, &prefixes),
                    t.title
                );
            });
        }
    }
    Ok(())
}

/// Describe an empty `task list` result, echoing the filters that were applied.
fn empty_task_note(filter: &ListFilter) -> String {
    let mut note = String::from("No ");
    if let Some(state) = &filter.state {
        note.push_str(&state.as_str().replace('_', "-"));
        note.push(' ');
    }
    note.push_str("tasks");
    if let Some(tag) = &filter.tag {
        note.push_str(" tagged #");
        note.push_str(tag);
    }
    if !filter.terms.0.is_empty() {
        note.push_str(" matching '");
        note.push_str(&filter.terms.to_string());
        note.push('\'');
    }
    note.push('.');
    note
}

fn print_task(t: &Task, prefixes: &HashMap<String, usize>) {
    println!(
        "{} [{}] {}",
        output::format_key_from_map(&t.key, prefixes),
        t.state,
        t.title
    );
}

/// Each listed task's tag names, by task id, without the tags `filter`
/// requires: every listed task carries those, so they're noise.
async fn listed_tags(
    conn: &mut SqliteConnection,
    tasks: &[Task],
    filter: &ListFilter,
) -> Result<HashMap<i64, Vec<String>>, Error> {
    let terms = Filter(filter.all_terms()?);
    let required: Vec<_> = terms.tags().collect();
    let mut by_task = HashMap::with_capacity(tasks.len());
    for t in tasks {
        let names = ops::tag::list_for_task(conn, t.id)
            .await?
            .into_iter()
            .filter(|tag| {
                !required
                    .iter()
                    .any(|r| r.key == tag.key && r.value == tag.value)
            })
            .map(|tag| tag.name)
            .collect();
        by_task.insert(t.id, names);
    }
    Ok(by_task)
}

/// Print `tasks`, which `ops::task::list` returns sorted by state, under a
/// header per state, each followed by its tags.
fn print_board(
    tasks: &[Task],
    prefixes: &HashMap<String, usize>,
    tags: &HashMap<i64, Vec<String>>,
) {
    for (i, group) in tasks.chunk_by(|a, b| a.state == b.state).enumerate() {
        if i > 0 {
            println!();
        }
        println!("[{}]", group[0].state);
        for t in group {
            let mut line = format!(
                "  {} {}",
                output::format_key_from_map(&t.key, prefixes),
                t.title
            );
            for name in tags.get(&t.id).into_iter().flatten() {
                line.push(' ');
                line.push_str(&output::format_tag(name));
            }
            println!("{line}");
        }
    }
}

fn print_task_detail(t: &Task, prefixes: &HashMap<String, usize>) {
    println!("Key:     {}", output::format_key_from_map(&t.key, prefixes));
    println!("Title:   {}", t.title);
    println!("State:   {}", t.state);
    if t.archived {
        println!("Archived: yes");
    }
    if let Some(desc) = &t.description {
        println!("Desc:    {}", desc);
    }
    println!("Created: {}", t.created_at);
    println!("Updated: {}", t.updated_at);
    if let Some(done_at) = &t.done_at {
        println!("Done:    {}", done_at);
    }
}

/// Open $EDITOR with the task fields, apply changes on save.
async fn edit_in_editor(
    conn: &mut SqliteConnection,
    editor: Option<&str>,
    task: &Task,
) -> Result<Task, Error> {
    let editor =
        editor.ok_or_else(|| Error::Usage("EDITOR environment variable not set".into()))?;

    let mut tmp = tempfile::Builder::new().prefix("ranger-edit-").tempfile()?;

    // Write current task: first line is title, rest is description.
    writeln!(tmp, "{}", task.title)?;
    if let Some(desc) = &task.description {
        writeln!(tmp, "{}", desc)?;
    }
    tmp.flush()?;

    // Open editor and wait for it to finish.
    let status = std::process::Command::new(editor)
        .arg(tmp.path())
        .status()?;

    if !status.success() {
        return Err(Error::Usage(format!(
            "editor '{}' exited with status {}",
            editor, status
        )));
    }

    // Parse the edited file.
    let edited = std::fs::read_to_string(tmp.path())?;
    let lines: Vec<&str> = edited.lines().collect();

    // Trim leading and trailing empty lines.
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());

    let (title, description) = match (first, last) {
        (Some(f), Some(l)) => {
            let title = lines[f].trim().to_string();
            // Drop the blank lines separating the title from the body; an
            // all-blank body clears the description.
            let body = &lines[f + 1..=l];
            let start = body.iter().position(|line| !line.trim().is_empty());
            let description = start.map(|s| body[s..].join("\n")).unwrap_or_default();
            (Some(title), Some(description))
        }
        _ => (None, None),
    };

    let updated = ops::task::edit(
        conn,
        task.id,
        title.as_deref(),
        description.as_deref(),
        None,
    )
    .await?;

    Ok(updated)
}
