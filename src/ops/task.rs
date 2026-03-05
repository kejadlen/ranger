use crate::error::RangerError;
use crate::key;
use crate::models::{State, Task};
use crate::position;
use sqlx::sqlite::SqliteConnection;

const TASK_COLUMNS: &str =
    "id, key, backlog_id, parent_id, title, description, state, position, created_at, updated_at";

pub struct CreateTask<'a> {
    pub title: &'a str,
    pub backlog_id: i64,
    pub state: Option<State>,
    pub parent_id: Option<i64>,
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

    let query = format!(
        "INSERT INTO tasks (key, backlog_id, parent_id, title, description, state, position) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING {TASK_COLUMNS}"
    );

    let task = sqlx::query_as::<_, Task>(&query)
        .bind(&key)
        .bind(params.backlog_id)
        .bind(params.parent_id)
        .bind(params.title)
        .bind(params.description)
        .bind(state.as_str())
        .bind(&new_pos)
        .fetch_one(&mut *conn)
        .await?;

    Ok(task)
}

pub async fn list(
    conn: &mut SqliteConnection,
    backlog_id: i64,
    state_filter: Option<State>,
) -> Result<Vec<Task>, RangerError> {
    let tasks = if let Some(state) = state_filter {
        let query = format!(
            "SELECT {TASK_COLUMNS} FROM tasks \
             WHERE backlog_id = ? AND state = ? \
             ORDER BY position"
        );
        sqlx::query_as::<_, Task>(&query)
            .bind(backlog_id)
            .bind(state.as_str())
            .fetch_all(&mut *conn)
            .await?
    } else {
        let query = format!(
            "SELECT {TASK_COLUMNS} FROM tasks \
             WHERE backlog_id = ? \
             ORDER BY position"
        );
        sqlx::query_as::<_, Task>(&query)
            .bind(backlog_id)
            .fetch_all(&mut *conn)
            .await?
    };
    Ok(tasks)
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
) -> Result<Task, RangerError> {
    let pattern = format!("{prefix}%");
    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE key LIKE ?");
    let matches = sqlx::query_as::<_, Task>(&query)
        .bind(&pattern)
        .fetch_all(&mut *conn)
        .await?;

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
    if let Some(state) = &state {
        sqlx::query("UPDATE tasks SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(state.as_str())
            .bind(task_id)
            .execute(&mut *conn)
            .await?;
    }

    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?");
    let task = sqlx::query_as::<_, Task>(&query)
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(task)
}

pub async fn move_task(
    conn: &mut SqliteConnection,
    task_id: i64,
    before_task_id: Option<i64>,
    after_task_id: Option<i64>,
) -> Result<(), RangerError> {
    // Look up the task's backlog_id so position queries are scoped correctly
    let backlog_id: i64 = sqlx::query_scalar("SELECT backlog_id FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;

    let upper: Option<String> = if let Some(id) = before_task_id {
        sqlx::query_scalar("SELECT position FROM tasks WHERE id = ? AND backlog_id = ?")
            .bind(id)
            .bind(backlog_id)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        None
    };

    let lower: Option<String> = if let Some(id) = after_task_id {
        sqlx::query_scalar("SELECT position FROM tasks WHERE id = ? AND backlog_id = ?")
            .bind(id)
            .bind(backlog_id)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        None
    };

    let new_pos = match (lower.as_deref(), upper.as_deref()) {
        (None, None) => {
            let last_pos: Option<String> = sqlx::query_scalar(
                "SELECT position FROM tasks \
                 WHERE backlog_id = ? \
                 ORDER BY position DESC LIMIT 1",
            )
            .bind(backlog_id)
            .fetch_optional(&mut *conn)
            .await?;
            position::between(last_pos.as_deref().unwrap_or(""), "")
        }
        (Some(low), None) => {
            let next: Option<String> = sqlx::query_scalar(
                "SELECT position FROM tasks \
                 WHERE backlog_id = ? AND id != ? AND position > ? \
                 ORDER BY position ASC LIMIT 1",
            )
            .bind(backlog_id)
            .bind(task_id)
            .bind(low)
            .fetch_optional(&mut *conn)
            .await?;
            position::between(low, next.as_deref().unwrap_or(""))
        }
        (None, Some(up)) => {
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT position FROM tasks \
                 WHERE backlog_id = ? AND id != ? AND position < ? \
                 ORDER BY position DESC LIMIT 1",
            )
            .bind(backlog_id)
            .bind(task_id)
            .bind(up)
            .fetch_optional(&mut *conn)
            .await?;
            position::between(prev.as_deref().unwrap_or(""), up)
        }
        (Some(low), Some(up)) => position::between(low, up),
    };

    sqlx::query("UPDATE tasks SET position = ? WHERE id = ?")
        .bind(&new_pos)
        .bind(task_id)
        .execute(&mut *conn)
        .await?;

    Ok(())
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
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let icebox = list(&mut conn, bl.id, Some(State::Icebox)).await.unwrap();
        assert_eq!(icebox.len(), 1);
        assert_eq!(icebox[0].title, "Icebox task");

        let queued = list(&mut conn, bl.id, Some(State::Queued)).await.unwrap();
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        let found = get_by_key_prefix(&mut conn, &task.key[..3]).await.unwrap();
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
                parent_id: None,
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
    async fn move_task_before() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move t3 before t1 — should produce order: t3, t1, t2
        move_task(&mut conn, t3.id, Some(t1.id), None)
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t3.id, "t3 should be first");
        assert_eq!(tasks[1].id, t1.id, "t1 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn move_task_after() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move t1 after t3 — should produce order: t2, t3, t1
        move_task(&mut conn, t1.id, None, Some(t3.id))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t2.id, "t2 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t1.id, "t1 should be third");
    }

    #[tokio::test]
    async fn move_task_after_into_middle() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move t3 after t1 (but before t2) — should produce order: t1, t3, t2
        move_task(&mut conn, t3.id, None, Some(t1.id))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second (after t1)");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn move_task_before_from_middle() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move t1 before t3 (but after t2) — should produce order: t2, t1, t3
        move_task(&mut conn, t1.id, Some(t3.id), None)
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t2.id, "t2 should be first");
        assert_eq!(tasks[1].id, t1.id, "t1 should be second (before t3)");
        assert_eq!(tasks[2].id, t3.id, "t3 should be third");
    }

    #[tokio::test]
    async fn move_task_between() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create(
            &mut conn,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move t3 after t1 and before t2 — should produce order: t1, t3, t2
        move_task(&mut conn, t3.id, Some(t2.id), Some(t1.id))
            .await
            .unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        delete(&mut conn, task.id).await.unwrap();

        let result = get_by_key_prefix(&mut conn, &task.key).await;
        assert!(result.is_err());

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
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
                parent_id: None,
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

        let result = get_by_key_prefix(&mut conn, "kkkk").await;
        assert!(result.is_err());
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
                parent_id: None,
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
                parent_id: None,
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
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        // Move first task to end (no before/after)
        move_task(&mut conn, t1.id, None, None).await.unwrap();

        let tasks = list(&mut conn, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t2.id);
        assert_eq!(tasks[1].id, t1.id);
    }
}
