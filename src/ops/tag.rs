use crate::error::RangerError;
use crate::models::Tag;
use sqlx::SqliteConnection;

/// Get or create a tag by name. Returns the existing tag if it already exists.
pub async fn get_or_create(conn: &mut SqliteConnection, name: &str) -> Result<Tag, RangerError> {
    if let Some(tag) = sqlx::query_as::<_, Tag>("SELECT id, name FROM tags WHERE name = ?")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?
    {
        return Ok(tag);
    }

    sqlx::query("INSERT INTO tags (name) VALUES (?)")
        .bind(name)
        .execute(&mut *conn)
        .await?;

    Ok(
        sqlx::query_as::<_, Tag>("SELECT id, name FROM tags WHERE name = ?")
            .bind(name)
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// Add a tag to a task. Creates the tag if it doesn't exist. No-op if already tagged.
pub async fn add(
    conn: &mut SqliteConnection,
    task_id: i64,
    name: &str,
) -> Result<Tag, RangerError> {
    let tag = get_or_create(conn, name).await?;

    sqlx::query("INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?, ?)")
        .bind(task_id)
        .bind(tag.id)
        .execute(&mut *conn)
        .await?;

    Ok(tag)
}

/// Remove a tag from a task.
pub async fn remove(
    conn: &mut SqliteConnection,
    task_id: i64,
    name: &str,
) -> Result<(), RangerError> {
    sqlx::query(
        "DELETE FROM task_tags WHERE task_id = ? AND tag_id = (SELECT id FROM tags WHERE name = ?)",
    )
    .bind(task_id)
    .bind(name)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// List all tags for a task.
pub async fn list_for_task(
    conn: &mut SqliteConnection,
    task_id: i64,
) -> Result<Vec<Tag>, RangerError> {
    Ok(sqlx::query_as::<_, Tag>(
        "SELECT t.id, t.name FROM tags t \
         JOIN task_tags tt ON t.id = tt.tag_id \
         WHERE tt.task_id = ? ORDER BY t.name",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?)
}

/// List all tags (globally).
pub async fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Tag>, RangerError> {
    Ok(
        sqlx::query_as::<_, Tag>("SELECT id, name FROM tags ORDER BY name")
            .fetch_all(&mut *conn)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::ops::task::CreateTask;

    async fn setup() -> (sqlx::SqlitePool, i64, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let backlog = ops::backlog::create(&mut conn, "Test").await.unwrap();
        let task = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Test task",
                backlog_id: backlog.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        (pool, task.id, dir)
    }

    #[tokio::test]
    async fn add_and_list_tags() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "frontend").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let tags = list_for_task(&mut conn, task_id).await.unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "bug");
        assert_eq!(tags[1].name, "frontend");
    }

    #[tokio::test]
    async fn add_duplicate_tag_is_noop() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "bug").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let tags = list_for_task(&mut conn, task_id).await.unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[tokio::test]
    async fn remove_tag() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "bug").await.unwrap();
        remove(&mut conn, task_id, "bug").await.unwrap();

        let tags = list_for_task(&mut conn, task_id).await.unwrap();
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_tag_is_noop() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        remove(&mut conn, task_id, "nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn list_all_tags() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "frontend").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let all = list_all(&mut conn).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "bug");
        assert_eq!(all[1].name, "frontend");
    }

    #[tokio::test]
    async fn tags_shared_across_tasks() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let backlog = ops::backlog::get_by_name(&mut conn, "Test").await.unwrap();
        let task2 = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Second task",
                backlog_id: backlog.id,
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        add(&mut conn, task_id, "shared").await.unwrap();
        add(&mut conn, task2.id, "shared").await.unwrap();

        let all = list_all(&mut conn).await.unwrap();
        assert_eq!(all.len(), 1); // One tag, used by two tasks

        let tags1 = list_for_task(&mut conn, task_id).await.unwrap();
        let tags2 = list_for_task(&mut conn, task2.id).await.unwrap();
        assert_eq!(tags1.len(), 1);
        assert_eq!(tags2.len(), 1);
        assert_eq!(tags1[0].name, "shared");
        assert_eq!(tags2[0].name, "shared");
    }
}
