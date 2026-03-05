use crate::error::RangerError;
use crate::key;
use crate::models::Backlog;
use sqlx::SqlitePool;

pub async fn create(pool: &SqlitePool, name: &str) -> Result<Backlog, RangerError> {
    // key column still exists in the schema but is unused; generate a dummy value
    let key = key::generate_key();
    let backlog = sqlx::query_as::<_, Backlog>(
        "INSERT INTO backlogs (key, name) VALUES (?, ?) RETURNING id, name, created_at, updated_at",
    )
    .bind(&key)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(backlog)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Backlog>, RangerError> {
    let backlogs = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(backlogs)
}

pub async fn get_by_name(pool: &SqlitePool, name: &str) -> Result<Backlog, RangerError> {
    let backlog = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| RangerError::KeyNotFound(name.to_string()))?;
    Ok(backlog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    async fn test_pool() -> SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_backlog() {
        let pool = test_pool().await;
        let backlog = create(&pool, "My Backlog").await.unwrap();
        assert_eq!(backlog.name, "My Backlog");

        let fetched = get_by_name(&pool, "My Backlog").await.unwrap();
        assert_eq!(fetched.id, backlog.id);
    }

    #[tokio::test]
    async fn list_backlogs() {
        let pool = test_pool().await;
        create(&pool, "First").await.unwrap();
        create(&pool, "Second").await.unwrap();

        let backlogs = list(&pool).await.unwrap();
        assert_eq!(backlogs.len(), 2);
    }

    #[tokio::test]
    async fn get_by_name_not_found() {
        let pool = test_pool().await;
        let result = get_by_name(&pool, "nonexistent").await;
        assert!(result.is_err());
    }
}
