use crate::error::RangerError;
use crate::key;
use crate::models::{State, Task};
use crate::position;
use sqlx::sqlite::SqliteConnection;

const TASK_COLUMNS: &str = "tasks.id, tasks.key, tasks.backlog_id, tasks.title, tasks.description, tasks.state, tasks.position, tasks.archived, tasks.created_at, tasks.updated_at, tasks.done_at";

pub struct CreateTask<'a> {
    pub title: &'a str,
    pub backlog_id: i64,
    pub state: Option<State>,
    pub description: Option<&'a str>,
}

pub async fn create(
    conn: &mut SqliteConnection,
    params: CreateTask<'_>,
) -> Result<Task, RangerError> {
    let key = key::generate_key();
    let state = params.state.unwrap_or(State::Icebox);

    let last_pos: Option<String> = sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? \
         ORDER BY position DESC LIMIT 1",
    )
    .bind(params.backlog_id)
    .fetch_optional(&mut *conn)
    .await?;

    let new_pos = position::between(last_pos.as_deref().unwrap_or(""), "");

    let done_at = if state == State::Done {
        Some("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
    } else {
        None
    };

    let query = format!(
        "INSERT INTO tasks (key, backlog_id, title, description, state, position, done_at) \
         VALUES (?, ?, ?, ?, ?, ?, {}) \
         RETURNING {TASK_COLUMNS}",
        done_at.unwrap_or("NULL")
    );

    let task = sqlx::query_as::<_, Task>(&query)
        .bind(&key)
        .bind(params.backlog_id)
        .bind(params.title)
        .bind(params.description)
        .bind(state.as_str())
        .bind(&new_pos)
        .fetch_one(&mut *conn)
        .await?;

    Ok(task)
}

#[derive(Default)]
pub struct ListFilter {
    pub state: Option<State>,
    pub include_archived: bool,
    pub tag: Option<String>,
}

pub async fn list(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    filter: &ListFilter,
) -> Result<Vec<Task>, RangerError> {
    let archived_clause = if filter.include_archived {
        ""
    } else {
        " AND archived = 0"
    };
    let tag_join = if filter.tag.is_some() {
        " JOIN task_tags tt ON tasks.id = tt.task_id \
         JOIN tags tg ON tt.tag_id = tg.id AND tg.name = ?"
    } else {
        ""
    };

    let is_done_only = filter.state.as_ref() == Some(&State::Done);
    let order_clause = if is_done_only {
        " ORDER BY done_at"
    } else {
        " ORDER BY position"
    };

    let tasks = if let Some(state) = &filter.state {
        let query = format!(
            "SELECT {TASK_COLUMNS} FROM tasks{tag_join} \
             WHERE backlog_id = ? AND state = ?{archived_clause}{order_clause}"
        );
        let mut q = sqlx::query_as::<_, Task>(&query);
        if let Some(tag) = &filter.tag {
            q = q.bind(tag);
        }
        q.bind(backlog_id)
            .bind(state.as_str())
            .fetch_all(&mut *conn)
            .await?
    } else {
        let query = format!(
            "SELECT {TASK_COLUMNS} FROM tasks{tag_join} \
             WHERE backlog_id = ?{archived_clause}{order_clause}"
        );
        let mut q = sqlx::query_as::<_, Task>(&query);
        if let Some(tag) = &filter.tag {
            q = q.bind(tag);
        }
        q.bind(backlog_id).fetch_all(&mut *conn).await?
    };

    Ok(tasks)
}

/// Fetch all task keys in the database. Used to compute shortest unique prefixes.
pub async fn all_keys(conn: &mut SqliteConnection) -> Result<Vec<String>, RangerError> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM tasks")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(k,)| k).collect())
}

pub async fn keys_for_backlog(
    conn: &mut SqliteConnection,
    backlog_id: i64,
) -> Result<Vec<String>, RangerError> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM tasks WHERE backlog_id = ?")
        .bind(backlog_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(k,)| k).collect())
}

pub async fn get_by_id(conn: &mut SqliteConnection, id: i64) -> Result<Task, RangerError> {
    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?");
    let task = sqlx::query_as::<_, Task>(&query)
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(task)
}

pub async fn get_by_key_prefix(
    conn: &mut SqliteConnection,
    prefix: &str,
    backlog_id: Option<i64>,
) -> Result<Task, RangerError> {
    let pattern = format!("{prefix}%");
    let matches = if let Some(bid) = backlog_id {
        let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE key LIKE ? AND backlog_id = ?");
        sqlx::query_as::<_, Task>(&query)
            .bind(&pattern)
            .bind(bid)
            .fetch_all(&mut *conn)
            .await?
    } else {
        let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE key LIKE ?");
        sqlx::query_as::<_, Task>(&query)
            .bind(&pattern)
            .fetch_all(&mut *conn)
            .await?
    };

    match matches.len() {
        0 => Err(RangerError::KeyNotFound(prefix.to_string())),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(RangerError::AmbiguousPrefix(prefix.to_string())),
    }
}

pub async fn edit(
    conn: &mut SqliteConnection,
    task_id: i64,
    title: Option<&str>,
    description: Option<&str>,
    state: Option<State>,
) -> Result<Task, RangerError> {
    if let Some(title) = title {
        sqlx::query("UPDATE tasks SET title = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(title)
            .bind(task_id)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(description) = description {
        sqlx::query("UPDATE tasks SET description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(description)
            .bind(task_id)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(new_state) = &state {
        // Fetch the current state to determine direction
        let old_state: State = sqlx::query_scalar("SELECT state FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&mut *conn)
            .await?;

        let done_at_expr = if *new_state == State::Done {
            "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"
        } else {
            "NULL"
        };
        let sql = format!(
            "UPDATE tasks SET state = ?, done_at = {done_at_expr}, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?"
        );
        sqlx::query(&sql)
            .bind(new_state.as_str())
            .bind(task_id)
            .execute(&mut *conn)
            .await?;

        if old_state != *new_state {
            reorder(&mut *conn, task_id, &old_state, new_state).await?;
        }
    }

    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?");
    let task = sqlx::query_as::<_, Task>(&query)
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(task)
}

pub async fn set_archived(
    conn: &mut SqliteConnection,
    task_id: i64,
    archived: bool,
) -> Result<Task, RangerError> {
    let query = format!(
        "UPDATE tasks SET archived = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
         WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let task = sqlx::query_as::<_, Task>(&query)
        .bind(archived)
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(task)
}

pub enum Placement<'a> {
    Before(&'a Task),
    After(&'a Task),
    Between { after: &'a Task, before: &'a Task },
}

impl<'a> Placement<'a> {
    pub fn anchors(&self) -> impl Iterator<Item = &Task> {
        match self {
            Placement::Before(t) | Placement::After(t) => vec![*t],
            Placement::Between { after, before } => vec![*after, *before],
        }
        .into_iter()
    }
}

pub async fn move_task(
    conn: &mut SqliteConnection,
    task: &Task,
    placement: Placement<'_>,
) -> Result<(), RangerError> {
    for anchor in placement.anchors() {
        if anchor.state != task.state {
            return Err(RangerError::StateMismatch {
                task_state: task.state.to_string(),
                anchor_state: anchor.state.to_string(),
            });
        }
    }

    let new_pos = match placement {
        Placement::After(anchor) => {
            let next =
                next_position_after(&mut *conn, task.backlog_id, task.id, &anchor.position).await?;
            position::between(&anchor.position, next.as_deref().unwrap_or(""))
        }
        Placement::Before(anchor) => {
            let prev = prev_position_before(&mut *conn, task.backlog_id, task.id, &anchor.position)
                .await?;
            position::between(prev.as_deref().unwrap_or(""), &anchor.position)
        }
        Placement::Between { after, before } => {
            position::between(&after.position, &before.position)
        }
    };

    set_position(&mut *conn, task.id, &new_pos).await
}

/// Reorder a task when its state changes.
///
/// Moving up (toward done): place at end of target state group.
/// Moving down (toward icebox): place at beginning of target state group.
async fn reorder(
    conn: &mut SqliteConnection,
    task_id: i64,
    old_state: &State,
    new_state: &State,
) -> Result<(), RangerError> {
    let backlog_id: i64 = sqlx::query_scalar("SELECT backlog_id FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;

    let moving_up = new_state.rank() > old_state.rank();

    let new_pos = if moving_up {
        let last_in_state =
            last_position_in_state(&mut *conn, backlog_id, task_id, new_state).await?;

        match last_in_state {
            Some(last) => {
                let next = next_position_after(&mut *conn, backlog_id, task_id, &last).await?;
                position::between(&last, next.as_deref().unwrap_or(""))
            }
            None => {
                let last = last_position(&mut *conn, backlog_id, task_id).await?;
                position::between(last.as_deref().unwrap_or(""), "")
            }
        }
    } else {
        let first_in_state =
            first_position_in_state(&mut *conn, backlog_id, task_id, new_state).await?;

        match first_in_state {
            Some(first) => {
                let prev = prev_position_before(&mut *conn, backlog_id, task_id, &first).await?;
                position::between(prev.as_deref().unwrap_or(""), &first)
            }
            None => {
                let first = first_position(&mut *conn, backlog_id, task_id).await?;
                position::between("", first.as_deref().unwrap_or(""))
            }
        }
    };

    set_position(&mut *conn, task_id, &new_pos).await
}

// -- Position query helpers --

async fn last_position(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND id != ? \
         ORDER BY position DESC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(exclude_task_id)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn first_position(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND id != ? \
         ORDER BY position ASC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(exclude_task_id)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn next_position_after(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
    pos: &str,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND id != ? AND position > ? \
         ORDER BY position ASC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(exclude_task_id)
    .bind(pos)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn prev_position_before(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
    pos: &str,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND id != ? AND position < ? \
         ORDER BY position DESC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(exclude_task_id)
    .bind(pos)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn last_position_in_state(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
    state: &State,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND state = ? AND id != ? \
         ORDER BY position DESC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(state.as_str())
    .bind(exclude_task_id)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn first_position_in_state(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    exclude_task_id: i64,
    state: &State,
) -> Result<Option<String>, RangerError> {
    Ok(sqlx::query_scalar(
        "SELECT position FROM tasks \
         WHERE backlog_id = ? AND state = ? AND id != ? \
         ORDER BY position ASC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(state.as_str())
    .bind(exclude_task_id)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn set_position(
    conn: &mut SqliteConnection,
    task_id: i64,
    pos: &str,
) -> Result<(), RangerError> {
    sqlx::query("UPDATE tasks SET position = ? WHERE id = ?")
        .bind(pos)
        .bind(task_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn rebalance(conn: &mut SqliteConnection, backlog_id: i64) -> Result<usize, RangerError> {
    let tasks = list(
        &mut *conn,
        backlog_id,
        &ListFilter {
            include_archived: true,
            ..Default::default()
        },
    )
    .await?;
    let positions = position::spread(tasks.len());

    for (task, pos) in tasks.iter().zip(&positions) {
        set_position(&mut *conn, task.id, pos).await?;
    }

    Ok(tasks.len())
}

pub async fn delete(conn: &mut SqliteConnection, task_id: i64) -> Result<(), RangerError> {
    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(task_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::State;
    use crate::ops::backlog;
    use tempfile::tempdir;

    async fn test_pool() -> sqlx::SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_task_in_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "My Task",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(task.title, "My Task");
        assert_eq!(task.state, State::Icebox);
        assert_eq!(task.backlog_id, bl.id);
        assert!(!task.key.is_empty());
        assert!(!task.position.is_empty());
    }

    #[tokio::test]
    async fn list_tasks_ordered_by_position() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &mut conn,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, t1.id);
        assert_eq!(tasks[1].id, t2.id);
        assert_eq!(tasks[2].id, t3.id);
    }

    #[tokio::test]
    async fn list_tasks_with_state_filter() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        create(
            &mut conn,
            CreateTask {
                title: "Icebox task",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        create(
            &mut conn,
            CreateTask {
                title: "Queued task",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        let icebox = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Icebox),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(icebox.len(), 1);
        assert_eq!(icebox[0].title, "Icebox task");

        let queued = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Queued),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].title, "Queued task");
    }

    #[tokio::test]
    async fn get_task_by_key_prefix() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "Find me",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let found = get_by_key_prefix(&mut conn, &task.key[..3], None)
            .await
            .unwrap();
        assert_eq!(found.id, task.id);
    }

    #[tokio::test]
    async fn edit_task_fields() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "Original",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let updated = edit(
            &mut conn,
            task.id,
            Some("Updated"),
            Some("A description"),
            Some(State::Queued),
        )
        .await
        .unwrap();

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.description.as_deref(), Some("A description"));
        assert_eq!(updated.state, State::Queued);
    }

    #[tokio::test]
    async fn delete_task() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "Delete me",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        delete(&mut conn, task.id).await.unwrap();

        let result = get_by_key_prefix(&mut conn, &task.key, None).await;
        assert!(result.is_err());

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn get_by_id_returns_task() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "Find by id",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let found = get_by_id(&mut conn, task.id).await.unwrap();
        assert_eq!(found.title, "Find by id");
    }

    #[tokio::test]
    async fn get_by_key_prefix_ambiguous() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        // Insert two tasks with keys that share a prefix, bypassing
        // the random key generator
        sqlx::query("INSERT INTO tasks (key, backlog_id, title, state, position) VALUES ('kkkkaaaa', ?, 'First', 'icebox', 'a')")
            .bind(bl.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tasks (key, backlog_id, title, state, position) VALUES ('kkkkbbbb', ?, 'Second', 'icebox', 'b')")
            .bind(bl.id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let result = get_by_key_prefix(&mut conn, "kkkk", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_by_key_prefix_scoped_to_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl1 = backlog::create(&mut conn, "Alpha").await.unwrap();
        let bl2 = backlog::create(&mut conn, "Beta").await.unwrap();

        // Two tasks with the same key prefix in different backlogs
        sqlx::query("INSERT INTO tasks (key, backlog_id, title, state, position) VALUES ('kkkkaaaa', ?, 'In Alpha', 'icebox', 'a')")
            .bind(bl1.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tasks (key, backlog_id, title, state, position) VALUES ('kkkkbbbb', ?, 'In Beta', 'icebox', 'a')")
            .bind(bl2.id)
            .execute(&mut *conn)
            .await
            .unwrap();

        // Globally ambiguous
        let result = get_by_key_prefix(&mut conn, "kkkk", None).await;
        assert!(result.is_err());

        // Scoped to each backlog resolves uniquely
        let t1 = get_by_key_prefix(&mut conn, "kkkk", Some(bl1.id))
            .await
            .unwrap();
        assert_eq!(t1.title, "In Alpha");

        let t2 = get_by_key_prefix(&mut conn, "kkkk", Some(bl2.id))
            .await
            .unwrap();
        assert_eq!(t2.title, "In Beta");
    }

    #[tokio::test]
    async fn edit_title_only() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let task = create(
            &mut conn,
            CreateTask {
                title: "Original",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let updated = edit(&mut conn, task.id, Some("New title"), None, None)
            .await
            .unwrap();
        assert_eq!(updated.title, "New title");
        assert_eq!(updated.state, State::Icebox);
    }

    #[tokio::test]
    async fn move_task_to_end() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move first task after second
        move_task(&mut conn, &t1, Placement::After(&t2))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks[0].id, t2.id);
        assert_eq!(tasks[1].id, t1.id);
    }

    #[tokio::test]
    async fn move_task_before() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "A",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "B",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &mut conn,
            CreateTask {
                title: "C",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        move_task(&mut conn, &t3, Placement::Before(&t1))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks[0].id, t3.id);
        assert_eq!(tasks[1].id, t1.id);
        assert_eq!(tasks[2].id, t2.id);
    }

    #[tokio::test]
    async fn move_task_after() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "A",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "B",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &mut conn,
            CreateTask {
                title: "C",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        move_task(&mut conn, &t1, Placement::After(&t3))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks[0].id, t2.id);
        assert_eq!(tasks[1].id, t3.id);
        assert_eq!(tasks[2].id, t1.id);
    }

    #[tokio::test]
    async fn move_task_between() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "A",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "B",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &mut conn,
            CreateTask {
                title: "C",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        move_task(
            &mut conn,
            &t3,
            Placement::Between {
                after: &t1,
                before: &t2,
            },
        )
        .await
        .unwrap();

        let tasks = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(tasks[0].id, t1.id);
        assert_eq!(tasks[1].id, t3.id);
        assert_eq!(tasks[2].id, t2.id);
    }

    #[tokio::test]
    async fn move_task_rejects_cross_state() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let queued = create(
            &mut conn,
            CreateTask {
                title: "Q",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let done = create(
            &mut conn,
            CreateTask {
                title: "D",
                backlog_id: bl.id,
                state: Some(State::Done),
                description: None,
            },
        )
        .await
        .unwrap();

        let err = move_task(&mut conn, &queued, Placement::Before(&done))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("queued"));
        assert!(err.to_string().contains("done"));
    }

    #[tokio::test]
    async fn state_change_up_places_at_end_of_target_group() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        // Create two done tasks and one queued task
        let d1 = create(
            &mut conn,
            CreateTask {
                title: "Done 1",
                backlog_id: bl.id,
                state: Some(State::Done),
                description: None,
            },
        )
        .await
        .unwrap();
        let d2 = create(
            &mut conn,
            CreateTask {
                title: "Done 2",
                backlog_id: bl.id,
                state: Some(State::Done),
                description: None,
            },
        )
        .await
        .unwrap();
        let q1 = create(
            &mut conn,
            CreateTask {
                title: "Queued 1",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        // Move queued task to done — should land after Done 2
        let updated = edit(&mut conn, q1.id, None, None, Some(State::Done))
            .await
            .unwrap();
        assert_eq!(updated.state, State::Done);

        let done = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Done),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(done.len(), 3);
        assert_eq!(done[0].id, d1.id);
        assert_eq!(done[1].id, d2.id);
        assert_eq!(
            done[2].id, q1.id,
            "newly done task should be at end of done group"
        );
    }

    #[tokio::test]
    async fn state_change_down_places_at_beginning_of_target_group() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        // Create two queued tasks and one in_progress task
        let q1 = create(
            &mut conn,
            CreateTask {
                title: "Queued 1",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let q2 = create(
            &mut conn,
            CreateTask {
                title: "Queued 2",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let ip = create(
            &mut conn,
            CreateTask {
                title: "In Progress",
                backlog_id: bl.id,
                state: Some(State::InProgress),
                description: None,
            },
        )
        .await
        .unwrap();

        // Move in_progress task to queued — should land before Queued 1
        let updated = edit(&mut conn, ip.id, None, None, Some(State::Queued))
            .await
            .unwrap();
        assert_eq!(updated.state, State::Queued);

        let queued = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Queued),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(queued.len(), 3);
        assert_eq!(
            queued[0].id, ip.id,
            "demoted task should be at beginning of queued group"
        );
        assert_eq!(queued[1].id, q1.id);
        assert_eq!(queued[2].id, q2.id);
    }

    #[tokio::test]
    async fn state_change_same_state_does_not_reorder() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        let original_pos = t1.position.clone();

        // Edit to same state — position should not change
        let updated = edit(&mut conn, t1.id, None, None, Some(State::Queued))
            .await
            .unwrap();
        assert_eq!(updated.position, original_pos);

        let queued = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Queued),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(queued[0].id, t1.id);
        assert_eq!(queued[1].id, t2.id);
    }

    #[tokio::test]
    async fn state_change_up_to_empty_group() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let t1 = create(
            &mut conn,
            CreateTask {
                title: "Queued task",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        // Move to done (empty group) — should succeed
        let updated = edit(&mut conn, t1.id, None, None, Some(State::Done))
            .await
            .unwrap();
        assert_eq!(updated.state, State::Done);

        let done = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Done),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, t1.id);
    }

    #[tokio::test]
    async fn state_change_down_to_empty_group() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let t1 = create(
            &mut conn,
            CreateTask {
                title: "In progress task",
                backlog_id: bl.id,
                state: Some(State::InProgress),
                description: None,
            },
        )
        .await
        .unwrap();

        // Move to icebox (empty group) — should succeed
        let updated = edit(&mut conn, t1.id, None, None, Some(State::Icebox))
            .await
            .unwrap();
        assert_eq!(updated.state, State::Icebox);

        let icebox = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Icebox),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(icebox.len(), 1);
        assert_eq!(icebox[0].id, t1.id);
    }

    #[tokio::test]
    async fn rebalance_reassigns_positions() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        // Create tasks — repeated appends make positions drift toward "z"
        for title in ["A", "B", "C"] {
            create(
                &mut conn,
                CreateTask {
                    title,
                    backlog_id: bl.id,
                    state: None,
                    description: None,
                },
            )
            .await
            .unwrap();
        }

        let before: Vec<String> = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap()
            .iter()
            .map(|t| t.position.clone())
            .collect();

        let count = rebalance(&mut conn, bl.id).await.unwrap();
        assert_eq!(count, 3);

        let after = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        let after_positions: Vec<String> = after.iter().map(|t| t.position.clone()).collect();

        // Order preserved
        assert_eq!(
            after.iter().map(|t| &t.title).collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );
        // Positions changed
        assert_ne!(before, after_positions);
        // Still sorted
        for w in after_positions.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    #[tokio::test]
    async fn rebalance_empty_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let count = rebalance(&mut conn, bl.id).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn keys_for_backlog_scoped_to_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl1 = backlog::create(&mut conn, "Alpha").await.unwrap();
        let bl2 = backlog::create(&mut conn, "Beta").await.unwrap();

        let t1 = create(
            &mut conn,
            CreateTask {
                title: "Task in Alpha",
                backlog_id: bl1.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Task in Beta",
                backlog_id: bl2.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let alpha_keys = keys_for_backlog(&mut conn, bl1.id).await.unwrap();
        assert_eq!(alpha_keys, vec![t1.key.clone()]);

        let beta_keys = keys_for_backlog(&mut conn, bl2.id).await.unwrap();
        assert_eq!(beta_keys, vec![t2.key.clone()]);

        let global_keys = all_keys(&mut conn).await.unwrap();
        assert_eq!(global_keys.len(), 2);
    }

    #[tokio::test]
    async fn set_archived_and_filter() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let t1 = create(
            &mut conn,
            CreateTask {
                title: "Keep",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Archive me",
                backlog_id: bl.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Archive t2
        let archived = set_archived(&mut conn, t2.id, true).await.unwrap();
        assert!(archived.archived);

        // Default list excludes archived
        let visible = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].key, t1.key);

        // include_archived shows all
        let all = list(
            &mut conn,
            bl.id,
            &ListFilter {
                include_archived: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(all.len(), 2);

        // Unarchive
        let restored = set_archived(&mut conn, t2.id, false).await.unwrap();
        assert!(!restored.archived);

        let visible = list(&mut conn, bl.id, &ListFilter::default())
            .await
            .unwrap();
        assert_eq!(visible.len(), 2);
    }

    #[tokio::test]
    async fn list_tasks_with_tag_and_state_filter() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "Tagged queued",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        create(
            &mut conn,
            CreateTask {
                title: "Untagged queued",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        crate::ops::tag::add(&mut conn, t1.id, "bug").await.unwrap();

        let results = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Queued),
                tag: Some("bug".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Tagged queued");
    }

    // ---- done_at tests ----

    #[tokio::test]
    async fn create_with_done_state_sets_done_at() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let task = create(
            &mut conn,
            CreateTask {
                title: "Done task",
                backlog_id: bl.id,
                state: Some(State::Done),
                description: None,
            },
        )
        .await
        .unwrap();

        assert!(task.done_at.is_some(), "done task should have done_at set");
    }

    #[tokio::test]
    async fn create_with_non_done_state_has_no_done_at() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let task = create(
            &mut conn,
            CreateTask {
                title: "Queued task",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        assert!(
            task.done_at.is_none(),
            "non-done task should not have done_at"
        );
    }

    #[tokio::test]
    async fn edit_to_done_sets_done_at() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let task = create(
            &mut conn,
            CreateTask {
                title: "Task",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        assert!(task.done_at.is_none());

        let updated = edit(&mut conn, task.id, None, None, Some(State::Done))
            .await
            .unwrap();
        assert!(
            updated.done_at.is_some(),
            "should set done_at on transition to done"
        );
    }

    #[tokio::test]
    async fn edit_from_done_clears_done_at() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        let task = create(
            &mut conn,
            CreateTask {
                title: "Task",
                backlog_id: bl.id,
                state: Some(State::Done),
                description: None,
            },
        )
        .await
        .unwrap();
        assert!(task.done_at.is_some());

        let updated = edit(&mut conn, task.id, None, None, Some(State::Queued))
            .await
            .unwrap();
        assert!(
            updated.done_at.is_none(),
            "should clear done_at on transition away from done"
        );
    }

    #[tokio::test]
    async fn done_tasks_ordered_by_done_at() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();

        // Create three queued tasks
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First done",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &mut conn,
            CreateTask {
                title: "Second done",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &mut conn,
            CreateTask {
                title: "Third done",
                backlog_id: bl.id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap();

        // Mark them done in a specific order: t3, t1, t2
        // Use direct SQL to set distinct done_at timestamps
        sqlx::query(
            "UPDATE tasks SET state = 'done', done_at = '2026-01-01T00:00:00Z' WHERE id = ?",
        )
        .bind(t3.id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE tasks SET state = 'done', done_at = '2026-01-02T00:00:00Z' WHERE id = ?",
        )
        .bind(t1.id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE tasks SET state = 'done', done_at = '2026-01-03T00:00:00Z' WHERE id = ?",
        )
        .bind(t2.id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let done = list(
            &mut conn,
            bl.id,
            &ListFilter {
                state: Some(State::Done),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(done.len(), 3);
        assert_eq!(done[0].id, t3.id, "earliest done_at first");
        assert_eq!(done[1].id, t1.id);
        assert_eq!(done[2].id, t2.id, "latest done_at last");
    }
}
