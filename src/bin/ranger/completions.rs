use std::ffi::OsStr;
use std::path::PathBuf;

use clap_complete::engine::CompletionCandidate;

/// Resolve the database path the same way `main` does: `RANGER_DB` env var, else XDG default.
fn resolve_db_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RANGER_DB") {
        return Some(PathBuf::from(path));
    }
    let xdg = xdg::BaseDirectories::with_prefix("ranger").ok()?;
    xdg.place_data_file("ranger.db").ok()
}

/// Run an async closure on a fresh single-threaded tokio runtime.
/// Returns `None` if the runtime can't be created.
fn block_on<F, T>(f: F) -> Option<T>
where
    F: std::future::Future<Output = Option<T>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?
        .block_on(f)
}

/// Complete task keys, returning full keys with the task title as help text.
pub fn complete_task_keys(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return vec![];
    };
    let Some(db_path) = resolve_db_path() else {
        return vec![];
    };
    if !db_path.exists() {
        return vec![];
    }

    block_on(async {
        let pool = ranger::db::connect(&db_path).await.ok()?;
        let mut conn = pool.acquire().await.ok()?;

        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT key, title, state FROM tasks ORDER BY key")
                .fetch_all(&mut *conn)
                .await
                .ok()?;

        Some(
            rows.into_iter()
                .filter(|(key, _, _)| key.starts_with(current))
                .map(|(key, title, state)| {
                    CompletionCandidate::new(key).help(Some(format!("[{state}] {title}").into()))
                })
                .collect(),
        )
    })
    .unwrap_or_default()
}

/// Complete backlog names.
pub fn complete_backlog_names(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return vec![];
    };
    let Some(db_path) = resolve_db_path() else {
        return vec![];
    };
    if !db_path.exists() {
        return vec![];
    }

    block_on(async {
        let pool = ranger::db::connect(&db_path).await.ok()?;
        let mut conn = pool.acquire().await.ok()?;

        let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM backlogs ORDER BY name")
            .fetch_all(&mut *conn)
            .await
            .ok()?;

        Some(
            rows.into_iter()
                .filter(|(name,)| name.starts_with(current))
                .map(|(name,)| CompletionCandidate::new(name))
                .collect(),
        )
    })
    .unwrap_or_default()
}
