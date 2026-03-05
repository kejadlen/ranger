use crate::error::RangerError;
use crate::models::Tag;
use sqlx::SqlitePool;

/// Get or create a tag by name.
pub async fn get_or_create(pool: &SqlitePool, name: &str) -> Result<Tag, RangerError> {
    // Try insert, ignore conflict
    sqlx::query("INSERT OR IGNORE INTO tags (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;

    let tag = sqlx::query_as::<_, Tag>("SELECT id, name FROM tags WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(tag)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Tag>, RangerError> {
    let tags = sqlx::query_as::<_, Tag>("SELECT id, name FROM tags ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(tags)
}

pub async fn add_to_task(pool: &SqlitePool, task_id: i64, tag_id: i64) -> Result<(), RangerError> {
    sqlx::query("INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?, ?)")
        .bind(task_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_for_task(pool: &SqlitePool, task_id: i64) -> Result<Vec<Tag>, RangerError> {
    let tags = sqlx::query_as::<_, Tag>(
        "SELECT t.id, t.name FROM tags t \
         JOIN task_tags tt ON tt.tag_id = t.id \
         WHERE tt.task_id = ? ORDER BY t.name",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::ops::{backlog, task};
    use tempfile::tempdir;

    async fn test_pool() -> SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn get_or_create_is_idempotent() {
        let pool = test_pool().await;
        let t1 = get_or_create(&pool, "urgent").await.unwrap();
        let t2 = get_or_create(&pool, "urgent").await.unwrap();
        assert_eq!(t1.id, t2.id);
    }

    #[tokio::test]
    async fn list_tags() {
        let pool = test_pool().await;
        get_or_create(&pool, "beta").await.unwrap();
        get_or_create(&pool, "alpha").await.unwrap();

        let tags = list(&pool).await.unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "alpha");
        assert_eq!(tags[1].name, "beta");
    }

    #[tokio::test]
    async fn add_tag_to_task_and_list() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t = task::create(
            &pool,
            task::CreateTask {
                title: "Task",
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
        let tag = get_or_create(&pool, "important").await.unwrap();

        add_to_task(&pool, t.id, tag.id).await.unwrap();

        let tags = list_for_task(&pool, t.id).await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "important");
    }

    #[tokio::test]
    async fn add_tag_to_task_is_idempotent() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t = task::create(
            &pool,
            task::CreateTask {
                title: "Task",
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
        let tag = get_or_create(&pool, "dup").await.unwrap();

        add_to_task(&pool, t.id, tag.id).await.unwrap();
        add_to_task(&pool, t.id, tag.id).await.unwrap();

        let tags = list_for_task(&pool, t.id).await.unwrap();
        assert_eq!(tags.len(), 1);
    }
}
