use crate::error::RangerError;
use crate::models::Backlog;
use sqlx::sqlite::SqliteConnection;

pub async fn create(conn: &mut SqliteConnection, name: &str) -> Result<Backlog, RangerError> {
    let backlog = sqlx::query_as::<_, Backlog>(
        "INSERT INTO backlogs (name) VALUES (?) RETURNING id, name, created_at, updated_at",
    )
    .bind(name)
    .fetch_one(&mut *conn)
    .await?;
    Ok(backlog)
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<Backlog>, RangerError> {
    let backlogs = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs ORDER BY name",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(backlogs)
}

pub async fn get_by_name(conn: &mut SqliteConnection, name: &str) -> Result<Backlog, RangerError> {
    let backlog = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| RangerError::BacklogNotFound(name.to_string()))?;
    Ok(backlog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    async fn test_pool() -> sqlx::SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let backlog = create(&mut conn, "My Backlog").await.unwrap();
        assert_eq!(backlog.name, "My Backlog");

        let fetched = get_by_name(&mut conn, "My Backlog").await.unwrap();
        assert_eq!(fetched.id, backlog.id);
    }

    #[tokio::test]
    async fn list_backlogs() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        create(&mut conn, "First").await.unwrap();
        create(&mut conn, "Second").await.unwrap();

        let backlogs = list(&mut conn).await.unwrap();
        assert_eq!(backlogs.len(), 2);
    }

    #[tokio::test]
    async fn get_by_name_not_found() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = get_by_name(&mut conn, "nonexistent").await;
        assert!(result.is_err());
    }
}
