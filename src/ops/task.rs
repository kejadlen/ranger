use crate::error::RangerError;
use crate::key;
use crate::models::{State, Task};
use crate::position;
use sqlx::SqlitePool;

pub struct CreateTask<'a> {
    pub title: &'a str,
    pub backlog_id: i64,
    pub state: Option<State>,
    pub parent_id: Option<i64>,
    pub description: Option<&'a str>,
    pub before_task_id: Option<i64>,
    pub after_task_id: Option<i64>,
}

pub async fn create(pool: &SqlitePool, params: CreateTask<'_>) -> Result<Task, RangerError> {
    let key = key::generate_key();
    let state = params.state.unwrap_or(State::Icebox);

    let task = sqlx::query_as::<_, Task>(
        "INSERT INTO tasks (key, parent_id, title, description, state) \
         VALUES (?, ?, ?, ?, ?) \
         RETURNING id, key, parent_id, title, description, state, created_at, updated_at",
    )
    .bind(&key)
    .bind(params.parent_id)
    .bind(params.title)
    .bind(params.description)
    .bind(state.as_str())
    .fetch_one(pool)
    .await?;

    let new_pos = resolve_position(
        pool,
        params.backlog_id,
        task.id,
        params.before_task_id,
        params.after_task_id,
    )
    .await?;

    sqlx::query("INSERT INTO backlog_tasks (backlog_id, task_id, position) VALUES (?, ?, ?)")
        .bind(params.backlog_id)
        .bind(task.id)
        .bind(&new_pos)
        .execute(pool)
        .await?;

    Ok(task)
}

pub async fn list(
    pool: &SqlitePool,
    backlog_id: i64,
    state_filter: Option<State>,
) -> Result<Vec<Task>, RangerError> {
    let tasks = if let Some(state) = state_filter {
        sqlx::query_as::<_, Task>(
            "SELECT t.id, t.key, t.parent_id, t.title, t.description, t.state, \
             t.created_at, t.updated_at \
             FROM tasks t \
             JOIN backlog_tasks bt ON bt.task_id = t.id \
             WHERE bt.backlog_id = ? AND t.state = ? \
             ORDER BY bt.position",
        )
        .bind(backlog_id)
        .bind(state.as_str())
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Task>(
            "SELECT t.id, t.key, t.parent_id, t.title, t.description, t.state, \
             t.created_at, t.updated_at \
             FROM tasks t \
             JOIN backlog_tasks bt ON bt.task_id = t.id \
             WHERE bt.backlog_id = ? \
             ORDER BY bt.position",
        )
        .bind(backlog_id)
        .fetch_all(pool)
        .await?
    };
    Ok(tasks)
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Task, RangerError> {
    let task = sqlx::query_as::<_, Task>(
        "SELECT id, key, parent_id, title, description, state, created_at, updated_at \
         FROM tasks WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(task)
}

pub async fn get_by_key_prefix(pool: &SqlitePool, prefix: &str) -> Result<Task, RangerError> {
    let pattern = format!("{prefix}%");
    let matches = sqlx::query_as::<_, Task>(
        "SELECT id, key, parent_id, title, description, state, created_at, updated_at \
         FROM tasks WHERE key LIKE ?",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    match matches.len() {
        0 => Err(RangerError::KeyNotFound(prefix.to_string())),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(RangerError::AmbiguousPrefix(prefix.to_string())),
    }
}

pub async fn edit(
    pool: &SqlitePool,
    task_id: i64,
    title: Option<&str>,
    description: Option<&str>,
    state: Option<State>,
) -> Result<Task, RangerError> {
    if let Some(title) = title {
        sqlx::query("UPDATE tasks SET title = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(title)
            .bind(task_id)
            .execute(pool)
            .await?;
    }
    if let Some(description) = description {
        sqlx::query("UPDATE tasks SET description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(description)
            .bind(task_id)
            .execute(pool)
            .await?;
    }
    if let Some(state) = &state {
        sqlx::query("UPDATE tasks SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(state.as_str())
            .bind(task_id)
            .execute(pool)
            .await?;
    }

    let task = sqlx::query_as::<_, Task>(
        "SELECT id, key, parent_id, title, description, state, created_at, updated_at \
         FROM tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    Ok(task)
}

/// Compute a position string for a task within a backlog.
///
/// When both bounds are None, appends after the last task in the backlog.
/// When `before_task_id` or `after_task_id` is given, positions relative
/// to those tasks. `task_id` is excluded from adjacent-position lookups
/// (relevant for moves where the task already has a position).
async fn resolve_position(
    pool: &SqlitePool,
    backlog_id: i64,
    task_id: i64,
    before_task_id: Option<i64>,
    after_task_id: Option<i64>,
) -> Result<String, RangerError> {
    if before_task_id.is_none() && after_task_id.is_none() {
        // Append to end of backlog
        let last_pos: Option<String> = sqlx::query_scalar(
            "SELECT bt.position FROM backlog_tasks bt \
             WHERE bt.backlog_id = ? \
             ORDER BY bt.position DESC LIMIT 1",
        )
        .bind(backlog_id)
        .fetch_optional(pool)
        .await?;

        return Ok(position::midpoint(last_pos.as_deref(), None));
    }

    // "before" task = the task we want to appear after us (upper bound)
    // "after" task = the task we want to appear before us (lower bound)
    let upper_bound: Option<String> = if let Some(id) = before_task_id {
        sqlx::query_scalar(
            "SELECT position FROM backlog_tasks WHERE backlog_id = ? AND task_id = ?",
        )
        .bind(backlog_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
    } else {
        None
    };

    let lower_bound: Option<String> = if let Some(id) = after_task_id {
        sqlx::query_scalar(
            "SELECT position FROM backlog_tasks WHERE backlog_id = ? AND task_id = ?",
        )
        .bind(backlog_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
    } else {
        None
    };

    // When only one bound is given, find the adjacent task's position
    // to avoid collisions with existing positions.
    let (lower, upper) = match (&lower_bound, &upper_bound) {
        (Some(low), None) => {
            // --after only: find the next task's position as upper bound
            let next: Option<String> = sqlx::query_scalar(
                "SELECT position FROM backlog_tasks \
                 WHERE backlog_id = ? AND task_id != ? AND position > ? \
                 ORDER BY position ASC LIMIT 1",
            )
            .bind(backlog_id)
            .bind(task_id)
            .bind(low)
            .fetch_optional(pool)
            .await?;
            (Some(low.clone()), next)
        }
        (None, Some(up)) => {
            // --before only: find the previous task's position as lower bound
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT position FROM backlog_tasks \
                 WHERE backlog_id = ? AND task_id != ? AND position < ? \
                 ORDER BY position DESC LIMIT 1",
            )
            .bind(backlog_id)
            .bind(task_id)
            .bind(up)
            .fetch_optional(pool)
            .await?;
            (prev, Some(up.clone()))
        }
        _ => (lower_bound.clone(), upper_bound.clone()),
    };

    Ok(position::midpoint(lower.as_deref(), upper.as_deref()))
}

pub async fn move_task(
    pool: &SqlitePool,
    task_id: i64,
    backlog_id: i64,
    before_task_id: Option<i64>,
    after_task_id: Option<i64>,
) -> Result<(), RangerError> {
    let new_pos =
        resolve_position(pool, backlog_id, task_id, before_task_id, after_task_id).await?;

    sqlx::query("UPDATE backlog_tasks SET position = ? WHERE backlog_id = ? AND task_id = ?")
        .bind(&new_pos)
        .bind(backlog_id)
        .bind(task_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn add_to_backlog(
    pool: &SqlitePool,
    task_id: i64,
    backlog_id: i64,
) -> Result<(), RangerError> {
    // Get the task's state to find proper position
    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await?;

    let last_pos: Option<String> = sqlx::query_scalar(
        "SELECT bt.position FROM backlog_tasks bt \
         JOIN tasks t ON t.id = bt.task_id \
         WHERE bt.backlog_id = ? AND t.state = ? \
         ORDER BY bt.position DESC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(&state)
    .fetch_optional(pool)
    .await?;

    let new_pos = position::midpoint(last_pos.as_deref(), None);

    sqlx::query("INSERT INTO backlog_tasks (backlog_id, task_id, position) VALUES (?, ?, ?)")
        .bind(backlog_id)
        .bind(task_id)
        .bind(&new_pos)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn remove_from_backlog(
    pool: &SqlitePool,
    task_id: i64,
    backlog_id: i64,
) -> Result<(), RangerError> {
    sqlx::query("DELETE FROM backlog_tasks WHERE backlog_id = ? AND task_id = ?")
        .bind(backlog_id)
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, task_id: i64) -> Result<(), RangerError> {
    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(task_id)
        .execute(pool)
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

    async fn test_pool() -> SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_task_in_backlog() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "My Task",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(task.title, "My Task");
        assert_eq!(task.state, State::Icebox);
        assert!(!task.key.is_empty());

        // Verify backlog_tasks join row exists
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM backlog_tasks WHERE backlog_id = ? AND task_id = ?",
        )
        .bind(bl.id)
        .bind(task.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn list_tasks_ordered_by_position() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, t1.id);
        assert_eq!(tasks[1].id, t2.id);
        assert_eq!(tasks[2].id, t3.id);
    }

    #[tokio::test]
    async fn list_tasks_with_state_filter() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        create(
            &pool,
            CreateTask {
                title: "Icebox task",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        create(
            &pool,
            CreateTask {
                title: "Queued task",
                backlog_id: bl.id,
                state: Some(State::Queued),
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        let icebox = list(&pool, bl.id, Some(State::Icebox)).await.unwrap();
        assert_eq!(icebox.len(), 1);
        assert_eq!(icebox[0].title, "Icebox task");

        let queued = list(&pool, bl.id, Some(State::Queued)).await.unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].title, "Queued task");
    }

    #[tokio::test]
    async fn get_task_by_key_prefix() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "Find me",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        let found = get_by_key_prefix(&pool, &task.key[..3]).await.unwrap();
        assert_eq!(found.id, task.id);
    }

    #[tokio::test]
    async fn edit_task_fields() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "Original",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        let updated = edit(
            &pool,
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
    async fn add_task_to_second_backlog() {
        let pool = test_pool().await;
        let bl1 = backlog::create(&pool, "First").await.unwrap();
        let bl2 = backlog::create(&pool, "Second").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "Shared",
                backlog_id: bl1.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        add_to_backlog(&pool, task.id, bl2.id).await.unwrap();

        let tasks1 = list(&pool, bl1.id, None).await.unwrap();
        let tasks2 = list(&pool, bl2.id, None).await.unwrap();
        assert_eq!(tasks1.len(), 1);
        assert_eq!(tasks2.len(), 1);
        assert_eq!(tasks1[0].id, tasks2[0].id);
    }

    #[tokio::test]
    async fn remove_task_from_backlog() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "Remove me",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        remove_from_backlog(&pool, task.id, bl.id).await.unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn move_task_before() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Move t3 before t1 — should produce order: t3, t1, t2
        move_task(&pool, t3.id, bl.id, Some(t1.id), None)
            .await
            .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t3.id, "t3 should be first");
        assert_eq!(tasks[1].id, t1.id, "t1 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn move_task_after() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Move t1 after t3 — should produce order: t2, t3, t1
        move_task(&pool, t1.id, bl.id, None, Some(t3.id))
            .await
            .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t2.id, "t2 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t1.id, "t1 should be third");
    }

    #[tokio::test]
    async fn move_task_after_into_middle() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Move t3 after t1 (but before t2) — should produce order: t1, t3, t2
        move_task(&pool, t3.id, bl.id, None, Some(t1.id))
            .await
            .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second (after t1)");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn move_task_before_from_middle() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Move t1 before t3 (but after t2) — should produce order: t2, t1, t3
        move_task(&pool, t1.id, bl.id, Some(t3.id), None)
            .await
            .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t2.id, "t2 should be first");
        assert_eq!(tasks[1].id, t1.id, "t1 should be second (before t3)");
        assert_eq!(tasks[2].id, t3.id, "t3 should be third");
    }

    #[tokio::test]
    async fn move_task_between() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t3 = create(
            &pool,
            CreateTask {
                title: "Third",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Move t3 after t1 and before t2 — should produce order: t1, t3, t2
        move_task(&pool, t3.id, bl.id, Some(t2.id), Some(t1.id))
            .await
            .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn delete_task() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(
            &pool,
            CreateTask {
                title: "Delete me",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        delete(&pool, task.id).await.unwrap();

        let result = get_by_key_prefix(&pool, &task.key).await;
        assert!(result.is_err());

        // backlog_tasks should be cleaned up by cascade
        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn create_task_before() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Create a task before t1 — should produce order: t3, t1, t2
        let t3 = create(
            &pool,
            CreateTask {
                title: "Before first",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: Some(t1.id),
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t3.id, "t3 should be first");
        assert_eq!(tasks[1].id, t1.id, "t1 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn create_task_after() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Create a task after t1 — should produce order: t1, t3, t2
        let t3 = create(
            &pool,
            CreateTask {
                title: "After first",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: Some(t1.id),
            },
        )
        .await
        .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }

    #[tokio::test]
    async fn create_task_between() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t1 = create(
            &pool,
            CreateTask {
                title: "First",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();
        let t2 = create(
            &pool,
            CreateTask {
                title: "Second",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: None,
                after_task_id: None,
            },
        )
        .await
        .unwrap();

        // Create a task after t1 and before t2 — should produce order: t1, t3, t2
        let t3 = create(
            &pool,
            CreateTask {
                title: "Between",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
                before_task_id: Some(t2.id),
                after_task_id: Some(t1.id),
            },
        )
        .await
        .unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks[0].id, t1.id, "t1 should be first");
        assert_eq!(tasks[1].id, t3.id, "t3 should be second");
        assert_eq!(tasks[2].id, t2.id, "t2 should be third");
    }
}
