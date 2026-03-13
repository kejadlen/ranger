use crate::error::RangerError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;

pub use sqlx::SqlitePool;
pub type SqliteConnection = sqlx::sqlite::SqliteConnection;

pub async fn connect(path: &Path) -> Result<SqlitePool, RangerError> {
    // path.parent() only returns None for empty paths, which aren't valid
    // DB paths. Callers always provide a filename within a directory.
    let parent = path
        .parent()
        .expect("database path must have a parent directory");
    std::fs::create_dir_all(parent)?;

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn connect_creates_db_and_runs_migrations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = connect(&db_path).await.unwrap();

        let result = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();

        let table_names: Vec<String> = result
            .iter()
            .map(|row| sqlx::Row::get(row, "name"))
            .collect();

        assert!(table_names.contains(&"backlogs".to_string()));
        assert!(table_names.contains(&"tasks".to_string()));
        assert!(table_names.contains(&"comments".to_string()));
        assert!(table_names.contains(&"tags".to_string()));
        assert!(table_names.contains(&"task_tags".to_string()));
        assert!(!table_names.contains(&"blockers".to_string()));
        assert!(!table_names.contains(&"backlog_tasks".to_string()));
    }
}
