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

        // Run migration 014 alone. The backfill must agree with the parse
        // that new tags get, and keep legacy tags that no longer parse.
        let pool = migrate_before(&db_path, 15).await;
        let expected = [
            ("bug", "", "bug"),
            ("project:ranger", "project", "ranger"),
            ("url:http://x", "url", "http://x"),
            (":x", "", "x"),
            ("x:", "x", ""),
        ];
        for (name, want_key, want_value) in expected {
            let (key, value): (String, String) =
                sqlx::query_as("SELECT key, value FROM tags WHERE name = ?")
                    .bind(name)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                (key.as_str(), value.as_str()),
                (want_key, want_value),
                "{name}"
            );
            if let Ok(parts) = crate::models::TagParts::try_from(name) {
                assert_eq!((parts.key, parts.value), (want_key, want_value), "{name}");
            }
        }
    }

    #[tokio::test]
    async fn drop_tag_name_migration_merges_duplicates_and_keeps_links() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = migrate_before(&db_path, 15).await;

        sqlx::raw_sql(
            "INSERT INTO backlogs (id, name) VALUES (1, 'b');
             INSERT INTO tasks (id, key, backlog_id, title, position) VALUES
                 (1, 'kkkk', 1, 'one', 'a'),
                 (2, 'llll', 1, 'two', 'b');
             -- `bug` and `:bug` share a pair; `x:` becomes bare `x`, which
             -- already exists. Task 1 is linked to both halves of each.
             INSERT INTO tags (id, name, key, value) VALUES
                 (1, 'bug', '', 'bug'),
                 (2, ':bug', '', 'bug'),
                 (3, 'x', '', 'x'),
                 (4, 'x:', 'x', ''),
                 (5, 'project:ranger', 'project', 'ranger');
             INSERT INTO task_tags (task_id, tag_id) VALUES
                 (1, 1), (1, 2), (2, 2),
                 (1, 3), (1, 4), (2, 4),
                 (2, 5);",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let pool = migrate_before(&db_path, 16).await;

        let tags: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT id, key, value FROM tags ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            tags,
            vec![
                (1, String::new(), "bug".to_string()),
                (3, String::new(), "x".to_string()),
                (5, "project".to_string(), "ranger".to_string()),
            ]
        );

        let links: Vec<(i64, i64)> =
            sqlx::query_as("SELECT task_id, tag_id FROM task_tags ORDER BY task_id, tag_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(links, vec![(1, 1), (1, 3), (2, 1), (2, 3), (2, 5)]);

        let fk_violations: Vec<(String,)> =
            sqlx::query_as("SELECT \"table\" FROM pragma_foreign_key_check")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(fk_violations.is_empty());
    }

    #[tokio::test]
    async fn backlogs_are_mirrored_into_project_tags() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = migrate_before(&db_path, 16).await;

        sqlx::raw_sql(
            "INSERT INTO backlogs (id, name) VALUES (1, 'ranger'), (2, 'dotfiles'), (3, 'empty');
             INSERT INTO tasks (id, key, backlog_id, title, position) VALUES
                 (1, 'kkkk', 1, 'one', 'a'),
                 (2, 'llll', 1, 'two', 'b'),
                 (3, 'mmmm', 2, 'three', 'a');
             -- One task already carries its project tag by hand.
             INSERT INTO tags (id, key, value) VALUES (1, 'project', 'ranger'), (2, '', 'bug');
             INSERT INTO task_tags (task_id, tag_id) VALUES (1, 1), (1, 2);",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let pool = connect(&db_path).await.unwrap();
        let links: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT t.key, tg.key, tg.value FROM task_tags tt \
             JOIN tasks t ON t.id = tt.task_id JOIN tags tg ON tg.id = tt.tag_id \
             ORDER BY t.key, tg.key, tg.value",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let s = |x: &str| x.to_string();
        assert_eq!(
            links,
            vec![
                (s("kkkk"), s(""), s("bug")),
                (s("kkkk"), s("project"), s("ranger")),
                (s("llll"), s("project"), s("ranger")),
                (s("mmmm"), s("project"), s("dotfiles")),
            ]
        );

        // The empty backlog gets its tag too, ready for its first task.
        let empty: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tags WHERE key = 'project' AND value = 'empty'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(empty.0, 1);
    }

    #[tokio::test]
    async fn dropping_backlog_id_keeps_order_comments_and_tags() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = migrate_before(&db_path, 17).await;

        // Positions collide across backlogs, and backlog 2 sorts first by
        // name but second by id.
        sqlx::raw_sql(
            "INSERT INTO backlogs (id, name) VALUES (1, 'zeta'), (2, 'alpha');
             INSERT INTO tasks (id, key, backlog_id, title, position) VALUES
                 (1, 'kkkk', 2, 'alpha second', 'n'),
                 (2, 'llll', 1, 'zeta first', 'b'),
                 (3, 'mmmm', 2, 'alpha first', 'b'),
                 (4, 'nnnn', 1, 'zeta second', 'nz');
             INSERT INTO tags (id, key, value) VALUES (1, '', 'bug');
             INSERT INTO task_tags (task_id, tag_id) VALUES (3, 1);
             INSERT INTO comments (task_id, body) VALUES (4, 'still here');",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let pool = connect(&db_path).await.unwrap();
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT title, position FROM tasks ORDER BY position")
                .fetch_all(&pool)
                .await
                .unwrap();
        let titles: Vec<&str> = rows.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            titles,
            ["zeta first", "zeta second", "alpha first", "alpha second"]
        );
        for (_, pos) in &rows {
            assert!(!pos.ends_with('a'), "{pos}");
        }

        let tagged: (i64,) = sqlx::query_as("SELECT task_id FROM task_tags WHERE tag_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(tagged.0, 3);
        let body: (String,) = sqlx::query_as("SELECT body FROM comments WHERE task_id = 4")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(body.0, "still here");
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
