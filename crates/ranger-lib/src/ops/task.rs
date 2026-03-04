use crate::error::RangerError;
use crate::key;
use crate::models::Task;
use crate::position;
use sqlx::SqlitePool;

pub async fn create(
    pool: &SqlitePool,
    title: &str,
    backlog_id: i64,
    state: Option<&str>,
    parent_id: Option<i64>,
    description: Option<&str>,
) -> Result<Task, RangerError> {
    let key = key::generate_key();
    let state = state.unwrap_or("icebox");

    let task = sqlx::query_as::<_, Task>(
        "INSERT INTO tasks (key, parent_id, title, description, state) \
         VALUES (?, ?, ?, ?, ?) \
         RETURNING id, key, parent_id, title, description, state, created_at, updated_at",
    )
    .bind(&key)
    .bind(parent_id)
    .bind(title)
    .bind(description)
    .bind(state)
    .fetch_one(pool)
    .await?;

    // Get the last position in this backlog+state to append
    let last_pos: Option<String> = sqlx::query_scalar(
        "SELECT bt.position FROM backlog_tasks bt \
         JOIN tasks t ON t.id = bt.task_id \
         WHERE bt.backlog_id = ? AND t.state = ? \
         ORDER BY bt.position DESC LIMIT 1",
    )
    .bind(backlog_id)
    .bind(state)
    .fetch_optional(pool)
    .await?;

    let new_pos = position::midpoint(last_pos.as_deref(), None);

    sqlx::query("INSERT INTO backlog_tasks (backlog_id, task_id, position) VALUES (?, ?, ?)")
        .bind(backlog_id)
        .bind(task.id)
        .bind(&new_pos)
        .execute(pool)
        .await?;

    Ok(task)
}

pub async fn list(
    pool: &SqlitePool,
    backlog_id: i64,
    state_filter: Option<&str>,
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
        .bind(state)
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
    state: Option<&str>,
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
    if let Some(state) = state {
        sqlx::query("UPDATE tasks SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?")
            .bind(state)
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

pub async fn move_task(
    pool: &SqlitePool,
    task_id: i64,
    backlog_id: i64,
    before_task_id: Option<i64>,
    after_task_id: Option<i64>,
) -> Result<(), RangerError> {
    let before_pos: Option<String> = if let Some(id) = before_task_id {
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

    let after_pos: Option<String> = if let Some(id) = after_task_id {
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

    let new_pos = position::midpoint(after_pos.as_deref(), before_pos.as_deref());

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
    let state: String =
        sqlx::query_scalar("SELECT state FROM tasks WHERE id = ?")
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
        let task = create(&pool, "My Task", bl.id, None, None, None)
            .await
            .unwrap();

        assert_eq!(task.title, "My Task");
        assert_eq!(task.state, "icebox");
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
        let t1 = create(&pool, "First", bl.id, None, None, None)
            .await
            .unwrap();
        let t2 = create(&pool, "Second", bl.id, None, None, None)
            .await
            .unwrap();
        let t3 = create(&pool, "Third", bl.id, None, None, None)
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
        create(&pool, "Icebox task", bl.id, None, None, None)
            .await
            .unwrap();
        create(&pool, "Queued task", bl.id, Some("queued"), None, None)
            .await
            .unwrap();

        let icebox = list(&pool, bl.id, Some("icebox")).await.unwrap();
        assert_eq!(icebox.len(), 1);
        assert_eq!(icebox[0].title, "Icebox task");

        let queued = list(&pool, bl.id, Some("queued")).await.unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].title, "Queued task");
    }

    #[tokio::test]
    async fn get_task_by_key_prefix() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(&pool, "Find me", bl.id, None, None, None)
            .await
            .unwrap();

        let found = get_by_key_prefix(&pool, &task.key[..3]).await.unwrap();
        assert_eq!(found.id, task.id);
    }

    #[tokio::test]
    async fn edit_task_fields() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(&pool, "Original", bl.id, None, None, None)
            .await
            .unwrap();

        let updated = edit(
            &pool,
            task.id,
            Some("Updated"),
            Some("A description"),
            Some("queued"),
        )
        .await
        .unwrap();

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.description.as_deref(), Some("A description"));
        assert_eq!(updated.state, "queued");
    }

    #[tokio::test]
    async fn add_task_to_second_backlog() {
        let pool = test_pool().await;
        let bl1 = backlog::create(&pool, "First").await.unwrap();
        let bl2 = backlog::create(&pool, "Second").await.unwrap();
        let task = create(&pool, "Shared", bl1.id, None, None, None)
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
        let task = create(&pool, "Remove me", bl.id, None, None, None)
            .await
            .unwrap();

        remove_from_backlog(&pool, task.id, bl.id).await.unwrap();

        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn delete_task() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let task = create(&pool, "Delete me", bl.id, None, None, None)
            .await
            .unwrap();

        delete(&pool, task.id).await.unwrap();

        let result = get_by_key_prefix(&pool, &task.key).await;
        assert!(result.is_err());

        // backlog_tasks should be cleaned up by cascade
        let tasks = list(&pool, bl.id, None).await.unwrap();
        assert_eq!(tasks.len(), 0);
    }
}
