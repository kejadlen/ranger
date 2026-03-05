use crate::error::RangerError;
use crate::models::Comment;
use sqlx::sqlite::SqliteConnection;

pub async fn add(
    conn: &mut SqliteConnection,
    task_id: i64,
    body: &str,
) -> Result<Comment, RangerError> {
    let comment = sqlx::query_as::<_, Comment>(
        "INSERT INTO comments (task_id, body) VALUES (?, ?) \
         RETURNING id, task_id, body, created_at",
    )
    .bind(task_id)
    .bind(body)
    .fetch_one(&mut *conn)
    .await?;
    Ok(comment)
}

pub async fn list(conn: &mut SqliteConnection, task_id: i64) -> Result<Vec<Comment>, RangerError> {
    let comments = sqlx::query_as::<_, Comment>(
        "SELECT id, task_id, body, created_at FROM comments WHERE task_id = ? ORDER BY created_at",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
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
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t = task::create(
            &mut conn,
            task::CreateTask {
                title: "Task",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        add(&mut conn, t.id, "First comment").await.unwrap();
        add(&mut conn, t.id, "Second comment").await.unwrap();

        let comments = list(&mut conn, t.id).await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "First comment");
        assert_eq!(comments[1].body, "Second comment");
    }
}
