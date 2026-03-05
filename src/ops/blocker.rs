use crate::error::RangerError;
use crate::models::Blocker;
use sqlx::sqlite::SqliteConnection;

pub async fn add(
    conn: &mut SqliteConnection,
    task_id: i64,
    blocked_by_task_id: i64,
) -> Result<Blocker, RangerError> {
    let blocker = sqlx::query_as::<_, Blocker>(
        "INSERT INTO blockers (task_id, blocked_by_task_id) VALUES (?, ?) \
         RETURNING id, task_id, blocked_by_task_id",
    )
    .bind(task_id)
    .bind(blocked_by_task_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(blocker)
}

pub async fn remove(
    conn: &mut SqliteConnection,
    task_id: i64,
    blocked_by_task_id: i64,
) -> Result<(), RangerError> {
    sqlx::query("DELETE FROM blockers WHERE task_id = ? AND blocked_by_task_id = ?")
        .bind(task_id)
        .bind(blocked_by_task_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn list_for_task(
    conn: &mut SqliteConnection,
    task_id: i64,
) -> Result<Vec<Blocker>, RangerError> {
    let blockers = sqlx::query_as::<_, Blocker>(
        "SELECT id, task_id, blocked_by_task_id FROM blockers WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(blockers)
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
    async fn add_and_list_blockers() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = task::create(
            &mut conn,
            task::CreateTask {
                title: "Blocked",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = task::create(
            &mut conn,
            task::CreateTask {
                title: "Blocker",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        add(&mut conn, t1.id, t2.id).await.unwrap();

        let blockers = list_for_task(&mut conn, t1.id).await.unwrap();
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].blocked_by_task_id, t2.id);
    }

    #[tokio::test]
    async fn remove_blocker() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = task::create(
            &mut conn,
            task::CreateTask {
                title: "Blocked",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = task::create(
            &mut conn,
            task::CreateTask {
                title: "Blocker",
                backlog_id: bl.id,
                state: None,
                parent_id: None,
                description: None,
            },
        )
        .await
        .unwrap();

        add(&mut conn, t1.id, t2.id).await.unwrap();
        remove(&mut conn, t1.id, t2.id).await.unwrap();

        let blockers = list_for_task(&mut conn, t1.id).await.unwrap();
        assert_eq!(blockers.len(), 0);
    }
}
