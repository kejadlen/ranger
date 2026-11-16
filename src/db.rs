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

    backup_before_migrate(path, &pool).await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Back up the database before running migrations, but only when the DB
/// already has tables and there are pending migrations to apply.
async fn backup_before_migrate(path: &Path, pool: &SqlitePool) -> Result<(), RangerError> {
    // Skip backup for brand-new databases (no tables yet).
    let table_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(pool)
    .await?;

    if table_count.0 == 0 {
        return Ok(());
    }

    let migrations = sqlx::migrate!();

    // Check which migrations have already been applied.
    let applied: Vec<(i64,)> = sqlx::query_as("SELECT version FROM _sqlx_migrations")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let applied_versions: std::collections::HashSet<i64> =
        applied.into_iter().map(|(v,)| v).collect();

    let has_pending = migrations
        .iter()
        .any(|m| !applied_versions.contains(&m.version));

    if !has_pending {
        return Ok(());
    }

    // Build backup path: <db>.YYYY-MM-DDTHHMMSS.bak
    let now = jiff::Zoned::now();
    let stamp = now.strftime("%Y-%m-%dT%H%M%S");
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".{stamp}.bak"));
    let backup_path = std::path::PathBuf::from(backup);

    let backup_str = backup_path
        .to_str()
        .expect("backup path must be valid UTF-8");
    sqlx::query(&format!("VACUUM INTO '{backup_str}'"))
        .execute(pool)
        .await?;

    eprintln!("Backed up database to {}", backup_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Open a database with only the migrations older than `version` applied.
    async fn migrate_before(path: &Path, version: i64) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let full = sqlx::migrate!();
        let older: Vec<_> = full
            .iter()
            .filter(|m| m.version < version)
            .cloned()
            .collect();
        let migrator = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(older),
            ..full
        };
        migrator.run(&pool).await.unwrap();
        pool
    }

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

    #[tokio::test]
    async fn tag_key_value_migration_backfills_existing_tags() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Build the schema as it was before migration 014, with existing tags.
        let pool = migrate_before(&db_path, 14).await;

        let names = ["bug", "project:ranger", "url:http://x", ":x", "x:"];
        for name in names {
            sqlx::query("INSERT INTO tags (name) VALUES (?)")
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;

        // Run the remaining migrations; the backfill must agree with the
        // parse that new tags get.
        let pool = connect(&db_path).await.unwrap();
        for name in names {
            let (key, value): (Option<String>, Option<String>) =
                sqlx::query_as("SELECT key, value FROM tags WHERE name = ?")
                    .bind(name)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let (want_key, want_value) = crate::ops::tag::parse(name);
            assert_eq!(key.as_deref(), want_key, "key for {name}");
            assert_eq!(value.as_deref(), want_value, "value for {name}");
        }
    }

    #[tokio::test]
    async fn no_backup_for_fresh_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let _pool = connect(&db_path).await.unwrap();

        // No .bak files should exist for a brand-new database
        let baks: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "bak"))
            .collect();
        assert!(baks.is_empty(), "fresh DB should not create a backup");
    }

    #[tokio::test]
    async fn backup_created_when_migrations_pending() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Apply every migration but the latest, leaving it pending
        let latest = sqlx::migrate!().iter().map(|m| m.version).max().unwrap();
        let pool = migrate_before(&db_path, latest).await;
        pool.close().await;

        // Reconnect — should detect pending migration and create backup
        let _pool2 = connect(&db_path).await.unwrap();

        let baks: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "bak"))
            .collect();
        assert_eq!(baks.len(), 1, "should create exactly one backup file");
    }

    #[tokio::test]
    async fn no_backup_when_already_up_to_date() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // First connect runs all migrations
        let pool = connect(&db_path).await.unwrap();
        pool.close().await;

        // Second connect — all migrations already applied, no backup
        let _pool2 = connect(&db_path).await.unwrap();

        let baks: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "bak"))
            .collect();
        assert!(baks.is_empty(), "up-to-date DB should not create a backup");
    }
}
