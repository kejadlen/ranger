use crate::error::RangerError;
use crate::models::Comment;
use sqlx::SqlitePool;

pub async fn add(pool: &SqlitePool, task_id: i64, body: &str) -> Result<Comment, RangerError> {
    let comment = sqlx::query_as::<_, Comment>(
        "INSERT INTO comments (task_id, body) VALUES (?, ?) \
         RETURNING id, task_id, body, created_at",
    )
    .bind(task_id)
    .bind(body)
    .fetch_one(pool)
    .await?;
    Ok(comment)
}

pub async fn list(pool: &SqlitePool, task_id: i64) -> Result<Vec<Comment>, RangerError> {
    let comments = sqlx::query_as::<_, Comment>(
        "SELECT id, task_id, body, created_at FROM comments WHERE task_id = ? ORDER BY created_at",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::ops::{backlog, task};
    use tempfile::tempdir;

    async fn test_pool() -> sqlx::SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn add_and_list_comments() {
        let pool = test_pool().await;
        let bl = backlog::create(&pool, "Test").await.unwrap();
        let t = task::create(&pool, "Task", bl.id, None, None, None)
            .await
            .unwrap();

        add(&pool, t.id, "First comment").await.unwrap();
        add(&pool, t.id, "Second comment").await.unwrap();

        let comments = list(&pool, t.id).await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "First comment");
        assert_eq!(comments[1].body, "Second comment");
    }
}
