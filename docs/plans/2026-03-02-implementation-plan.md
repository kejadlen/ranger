# Ranger First Milestone Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A working CLI that can create backlogs, manage tasks, and persist to SQLite — enough for an AI agent to use Ranger for its own task management.

**Architecture:** Cargo workspace with `ranger-lib` (core logic + DB) and `ranger-cli` (clap binary). SQLite via sqlx with offline compile-time checking. Async with tokio.

**Tech Stack:** Rust nightly, sqlx (sqlite + runtime-tokio), clap (derive), tokio, xdg, serde + serde_json, rand

---

### Task 1: Scaffold the Cargo workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/ranger-lib/Cargo.toml`
- Create: `crates/ranger-lib/src/lib.rs`
- Create: `crates/ranger-cli/Cargo.toml`
- Create: `crates/ranger-cli/src/main.rs`

**Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
members = ["crates/ranger-lib", "crates/ranger-cli"]
resolver = "2"
```

**Step 2: Create ranger-lib crate**

```toml
# crates/ranger-lib/Cargo.toml
[package]
name = "ranger-lib"
version = "0.1.0"
edition = "2024"

[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rand = "0.9"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
```

```rust
// crates/ranger-lib/src/lib.rs
pub fn hello() -> &'static str {
    "ranger"
}
```

**Step 3: Create ranger-cli crate**

```toml
# crates/ranger-cli/Cargo.toml
[package]
name = "ranger-cli"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "ranger"
path = "src/main.rs"

[dependencies]
ranger-lib = { path = "../ranger-lib" }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
xdg = "2"
```

```rust
// crates/ranger-cli/src/main.rs
fn main() {
    println!("{}", ranger_lib::hello());
}
```

**Step 4: Verify it builds and runs**

Run: `cargo build`
Run: `cargo run --bin ranger`
Expected: prints "ranger"

**Step 5: Commit**

Message: `Scaffold Cargo workspace with ranger-lib and ranger-cli`

---

### Task 2: Key generation

Implement jj-style key generation — random strings from a pronounceable alphabet, with prefix-based lookup.

**Files:**
- Create: `crates/ranger-lib/src/key.rs`
- Modify: `crates/ranger-lib/src/lib.rs`

**Step 1: Write tests for key generation**

```rust
// crates/ranger-lib/src/key.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_correct_length() {
        let key = generate_key();
        assert_eq!(key.len(), 16);
    }

    #[test]
    fn generated_key_uses_valid_alphabet() {
        let key = generate_key();
        for ch in key.chars() {
            assert!(
                ALPHABET.contains(&ch),
                "key contains invalid character: {ch}"
            );
        }
    }

    #[test]
    fn generated_keys_are_unique() {
        let keys: Vec<String> = (0..100).map(|_| generate_key()).collect();
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn resolve_prefix_exact_match() {
        let keys = vec!["romoqtuw".to_string(), "rypqxnkl".to_string()];
        assert_eq!(resolve_prefix("rom", &keys).unwrap(), "romoqtuw");
    }

    #[test]
    fn resolve_prefix_ambiguous() {
        let keys = vec!["romoqtuw".to_string(), "romxnklp".to_string()];
        assert!(resolve_prefix("rom", &keys).is_err());
    }

    #[test]
    fn resolve_prefix_no_match() {
        let keys = vec!["romoqtuw".to_string()];
        assert!(resolve_prefix("xyz", &keys).is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p ranger-lib`
Expected: compilation errors (functions don't exist yet)

**Step 3: Implement key generation and prefix resolution**

```rust
// crates/ranger-lib/src/key.rs
use rand::Rng;
use crate::error::RangerError;

/// jj-style alphabet: alternating consonant-vowel for pronounceability.
const ALPHABET: &[char] = &[
    'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

const KEY_LENGTH: usize = 16;

pub fn generate_key() -> String {
    let mut rng = rand::rng();
    (0..KEY_LENGTH)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx]
        })
        .collect()
}

pub fn resolve_prefix(prefix: &str, keys: &[String]) -> Result<String, RangerError> {
    let matches: Vec<&String> = keys.iter().filter(|k| k.starts_with(prefix)).collect();
    match matches.len() {
        0 => Err(RangerError::KeyNotFound(prefix.to_string())),
        1 => Ok(matches[0].clone()),
        _ => Err(RangerError::AmbiguousPrefix(prefix.to_string())),
    }
}
```

**Step 4: Create the error module**

```rust
// crates/ranger-lib/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum RangerError {
    #[error("no key matching prefix '{0}'")]
    KeyNotFound(String),
    #[error("ambiguous prefix '{0}' matches multiple keys")]
    AmbiguousPrefix(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}
```

**Step 5: Wire up lib.rs**

```rust
// crates/ranger-lib/src/lib.rs
pub mod error;
pub mod key;
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 7: Commit**

Message: `Add key generation and prefix resolution`

---

### Task 3: Lexicographic positioning

**Files:**
- Create: `crates/ranger-lib/src/position.rs`
- Modify: `crates/ranger-lib/src/lib.rs`

**Step 1: Write tests**

```rust
// crates/ranger-lib/src/position.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_position() {
        let pos = midpoint(None, None);
        assert_eq!(pos, "m");
    }

    #[test]
    fn before_existing() {
        let pos = midpoint(None, Some("m"));
        assert!(pos < *"m");
    }

    #[test]
    fn after_existing() {
        let pos = midpoint(Some("m"), None);
        assert!(pos > *"m");
    }

    #[test]
    fn between_two() {
        let pos = midpoint(Some("a"), Some("z"));
        assert!(pos > *"a");
        assert!(pos < *"z");
    }

    #[test]
    fn between_adjacent() {
        let pos = midpoint(Some("a"), Some("b"));
        assert!(pos > *"a");
        assert!(pos < *"b");
    }

    #[test]
    fn ordering_is_stable_over_many_inserts() {
        let mut positions = vec![midpoint(None, None)];
        // Insert 20 items at the end
        for _ in 0..20 {
            let last = positions.last().unwrap().clone();
            positions.push(midpoint(Some(&last), None));
        }
        for window in positions.windows(2) {
            assert!(window[0] < window[1], "{} should be < {}", window[0], window[1]);
        }
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p ranger-lib`
Expected: compilation errors

**Step 3: Implement midpoint**

Use a simple fractional indexing approach: operate on the string as a base-26 number (a-z), find the midpoint, extending length when needed.

```rust
// crates/ranger-lib/src/position.rs

/// Generate a position string between `before` and `after`.
/// Both are optional: None means "the boundary" (start or end).
pub fn midpoint(before: Option<&str>, after: Option<&str>) -> String {
    match (before, after) {
        (None, None) => "m".to_string(),
        (None, Some(after)) => midpoint_before(after),
        (Some(before), None) => midpoint_after(before),
        (Some(before), Some(after)) => midpoint_between(before, after),
    }
}

fn midpoint_before(s: &str) -> String {
    midpoint_between("a", s)
}

fn midpoint_after(s: &str) -> String {
    midpoint_between(s, "z")
}

fn midpoint_between(a: &str, b: &str) -> String {
    let a_digits: Vec<u8> = a.bytes().map(|b| b - b'a').collect();
    let b_digits: Vec<u8> = b.bytes().map(|b| b - b'a').collect();

    // Pad to same length, a with 0s (a), b with 25s (z)
    let len = a_digits.len().max(b_digits.len());
    let mut a_padded = a_digits.clone();
    let mut b_padded = b_digits.clone();
    a_padded.resize(len, 0);
    b_padded.resize(len, 25);

    // Convert to single number in base 26, find midpoint
    // Work digit by digit to avoid overflow
    let mut result = Vec::with_capacity(len + 1);
    let mut carry = 0i32;

    // Sum a + b digit by digit from least significant
    let mut sum_digits: Vec<i32> = Vec::with_capacity(len);
    for i in (0..len).rev() {
        let s = a_padded[i] as i32 + b_padded[i] as i32 + carry;
        sum_digits.push(s % 26);
        carry = s / 26;
    }
    if carry > 0 {
        sum_digits.push(carry);
    }
    sum_digits.reverse();

    // Divide by 2
    let mut remainder = 0i32;
    for digit in &sum_digits {
        let d = remainder * 26 + digit;
        result.push((d / 2) as u8);
        remainder = d % 2;
    }

    // If there's a remainder, add one more digit
    if remainder > 0 {
        result.push((remainder * 26 / 2) as u8);
    }

    // Trim trailing zeros (like 'a's), but keep at least one char
    while result.len() > 1 && *result.last().unwrap() == 0 {
        result.pop();
    }

    let s: String = result.iter().map(|&d| (d + b'a') as char).collect();
    s
}
```

**Step 4: Add module to lib.rs**

```rust
// crates/ranger-lib/src/lib.rs
pub mod error;
pub mod key;
pub mod position;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 6: Commit**

Message: `Add lexicographic position generation`

---

### Task 4: Database schema and migrations

**Files:**
- Create: `crates/ranger-lib/migrations/001_initial.sql`
- Create: `crates/ranger-lib/src/db.rs`
- Modify: `crates/ranger-lib/src/lib.rs`
- Modify: `crates/ranger-lib/src/error.rs`

**Step 1: Write the initial migration**

```sql
-- crates/ranger-lib/migrations/001_initial.sql
CREATE TABLE backlogs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    parent_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL DEFAULT 'icebox' CHECK(state IN ('icebox', 'queued', 'in_progress', 'done')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE backlog_tasks (
    backlog_id INTEGER NOT NULL REFERENCES backlogs(id) ON DELETE CASCADE,
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    position TEXT NOT NULL,
    PRIMARY KEY (backlog_id, task_id)
);

CREATE TABLE comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE blockers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    blocked_by_task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id, blocked_by_task_id)
);

CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE task_tags (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);
```

**Step 2: Implement the database connection and migration runner**

```rust
// crates/ranger-lib/src/db.rs
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use crate::error::RangerError;

pub async fn connect(path: &Path) -> Result<SqlitePool, RangerError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> Result<(), RangerError> {
    sqlx::query(include_str!("../migrations/001_initial.sql"))
        .execute(pool)
        .await?;
    Ok(())
}
```

**Step 3: Update error.rs to include IO errors**

Add `#[error("io error: {0}")] Io(#[from] std::io::Error)` variant.

**Step 4: Wire up lib.rs**

```rust
pub mod db;
pub mod error;
pub mod key;
pub mod position;
```

**Step 5: Write a test that connects and migrates**

```rust
// in db.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn connect_creates_db_and_runs_migrations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = connect(&db_path).await.unwrap();

        // Verify tables exist
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
        assert!(table_names.contains(&"backlog_tasks".to_string()));
        assert!(table_names.contains(&"comments".to_string()));
        assert!(table_names.contains(&"blockers".to_string()));
        assert!(table_names.contains(&"tags".to_string()));
        assert!(table_names.contains(&"task_tags".to_string()));
    }
}
```

Add `tempfile = "3"` to ranger-lib dev-dependencies.

**Step 6: Run tests**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 7: Commit**

Message: `Add database schema and migration`

---

### Task 5: Core models and types

**Files:**
- Create: `crates/ranger-lib/src/models.rs`
- Modify: `crates/ranger-lib/src/lib.rs`

**Step 1: Define the model types**

```rust
// crates/ranger-lib/src/models.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Icebox,
    Queued,
    InProgress,
    Done,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Icebox => "icebox",
            State::Queued => "queued",
            State::InProgress => "in_progress",
            State::Done => "done",
        }
    }
}

impl std::str::FromStr for State {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "icebox" => Ok(State::Icebox),
            "queued" => Ok(State::Queued),
            "in_progress" => Ok(State::InProgress),
            "done" => Ok(State::Done),
            _ => Err(format!("invalid state: {s}")),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backlog {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub key: String,
    pub parent_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocker {
    pub id: i64,
    pub task_id: i64,
    pub blocked_by_task_id: i64,
}
```

**Step 2: Add module to lib.rs**

**Step 3: Run `cargo check -p ranger-lib`**

Expected: compiles

**Step 4: Commit**

Message: `Add core model types`

---

### Task 6: Backlog CRUD operations

**Files:**
- Create: `crates/ranger-lib/src/ops/mod.rs`
- Create: `crates/ranger-lib/src/ops/backlog.rs`
- Modify: `crates/ranger-lib/src/lib.rs`

**Step 1: Write tests for backlog operations**

```rust
// crates/ranger-lib/src/ops/backlog.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    async fn test_pool() -> SqlitePool {
        let dir = tempdir().unwrap();
        // Leak the tempdir so it doesn't get cleaned up during test
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_backlog() {
        let pool = test_pool().await;
        let backlog = create(&pool, "My Backlog").await.unwrap();
        assert_eq!(backlog.name, "My Backlog");
        assert!(!backlog.key.is_empty());

        let fetched = get_by_key_prefix(&pool, &backlog.key[..3]).await.unwrap();
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
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p ranger-lib`

**Step 3: Implement backlog operations**

```rust
// crates/ranger-lib/src/ops/backlog.rs
use sqlx::SqlitePool;
use crate::error::RangerError;
use crate::key;
use crate::models::Backlog;

pub async fn create(pool: &SqlitePool, name: &str) -> Result<Backlog, RangerError> {
    let key = key::generate_key();
    let backlog = sqlx::query_as!(
        Backlog,
        r#"INSERT INTO backlogs (key, name) VALUES (?, ?) RETURNING id, key, name, created_at, updated_at"#,
        key,
        name
    )
    .fetch_one(pool)
    .await?;
    Ok(backlog)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Backlog>, RangerError> {
    let backlogs = sqlx::query_as!(Backlog, "SELECT id, key, name, created_at, updated_at FROM backlogs ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(backlogs)
}

pub async fn get_by_key_prefix(pool: &SqlitePool, prefix: &str) -> Result<Backlog, RangerError> {
    let pattern = format!("{prefix}%");
    let matches = sqlx::query_as!(
        Backlog,
        "SELECT id, key, name, created_at, updated_at FROM backlogs WHERE key LIKE ?",
        pattern
    )
    .fetch_all(pool)
    .await?;

    match matches.len() {
        0 => Err(RangerError::KeyNotFound(prefix.to_string())),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(RangerError::AmbiguousPrefix(prefix.to_string())),
    }
}
```

```rust
// crates/ranger-lib/src/ops/mod.rs
pub mod backlog;
```

**Step 4: Run tests**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 5: Commit**

Message: `Add backlog CRUD operations`

---

### Task 7: Task CRUD operations

**Files:**
- Create: `crates/ranger-lib/src/ops/task.rs`
- Modify: `crates/ranger-lib/src/ops/mod.rs`

**Step 1: Write tests for task operations**

Tests should cover:
- Create a task in a backlog (verifies task row + backlog_tasks join row + position)
- List tasks by backlog (ordered by position)
- List tasks by backlog and state filter
- Get task by key prefix
- Edit task title, description, state
- Add task to a second backlog
- Remove task from a backlog
- Delete a task

**Step 2: Run tests to verify they fail**

**Step 3: Implement task operations**

Key functions:
- `create(pool, title, backlog_id, state, parent_id, description) -> Task`
  - Generates key, inserts task, inserts backlog_tasks with position at end
- `list(pool, backlog_id, state_filter) -> Vec<Task>`
  - Joins backlog_tasks, ordered by position
- `get_by_key_prefix(pool, prefix) -> Task`
  - Same LIKE pattern as backlog
- `edit(pool, task_id, title, description, state) -> Task`
  - Updates only provided fields, updates `updated_at`
- `move_task(pool, task_id, backlog_id, before_key, after_key)`
  - Calculates new position using `position::midpoint`
- `add_to_backlog(pool, task_id, backlog_id)`
  - Inserts backlog_tasks row with position at end
- `remove_from_backlog(pool, task_id, backlog_id)`
- `delete(pool, task_id)`

**Step 4: Run tests**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 5: Commit**

Message: `Add task CRUD operations`

---

### Task 8: Comment, tag, and blocker operations

**Files:**
- Create: `crates/ranger-lib/src/ops/comment.rs`
- Create: `crates/ranger-lib/src/ops/tag.rs`
- Create: `crates/ranger-lib/src/ops/blocker.rs`
- Modify: `crates/ranger-lib/src/ops/mod.rs`

**Step 1: Write tests for each**

Comments: add, list by task
Tags: create-or-get, list, add to task, list task's tags
Blockers: add, remove, list by task

**Step 2: Run tests to verify they fail**

**Step 3: Implement each module**

**Step 4: Run tests**

Run: `cargo test -p ranger-lib`
Expected: all tests pass

**Step 5: Commit**

Message: `Add comment, tag, and blocker operations`

---

### Task 9: CLI scaffolding and backlog commands

**Files:**
- Modify: `crates/ranger-cli/src/main.rs`
- Create: `crates/ranger-cli/src/commands/mod.rs`
- Create: `crates/ranger-cli/src/commands/backlog.rs`
- Create: `crates/ranger-cli/src/output.rs`

**Step 1: Set up the clap CLI structure**

```rust
// crates/ranger-cli/src/main.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ranger", about = "Personal task tracker")]
struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to database file (default: $XDG_DATA_HOME/ranger/ranger.db)
    #[arg(long, env = "RANGER_DB", global = true)]
    db: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage backlogs
    Backlog {
        #[command(subcommand)]
        command: BacklogCommands,
    },
    // Task, Comment, Tag, Blocker subcommands added in later tasks
}
```

**Step 2: Implement DB path resolution**

Use `xdg::BaseDirectories` to find `$XDG_DATA_HOME/ranger/ranger.db`, with `--db` / `RANGER_DB` override.

**Step 3: Implement output module**

A helper that prints human-readable or JSON based on the `--json` flag.

**Step 4: Implement backlog subcommands** (create, list, show)

**Step 5: Test manually**

Run: `cargo run --bin ranger -- backlog create "Ranger"`
Run: `cargo run --bin ranger -- backlog list`
Run: `cargo run --bin ranger -- backlog list --json`

**Step 6: Commit**

Message: `Add CLI scaffolding and backlog commands`

---

### Task 10: CLI task commands

**Files:**
- Create: `crates/ranger-cli/src/commands/task.rs`
- Modify: `crates/ranger-cli/src/main.rs`

**Step 1: Add task subcommands**

Commands: create, list, show, edit, move, add, remove, delete

**Step 2: Implement each subcommand handler**

Each handler: parse args → call ranger-lib op → format output (human or JSON)

**Step 3: Test manually**

```
cargo run --bin ranger -- task create "Set up CI" --backlog <key>
cargo run --bin ranger -- task list --backlog <key>
cargo run --bin ranger -- task show <key>
cargo run --bin ranger -- task edit <key> --state queued
cargo run --bin ranger -- task list --backlog <key> --json
```

**Step 4: Commit**

Message: `Add CLI task commands`

---

### Task 11: CLI comment, tag, and blocker commands

**Files:**
- Create: `crates/ranger-cli/src/commands/comment.rs`
- Create: `crates/ranger-cli/src/commands/tag.rs`
- Create: `crates/ranger-cli/src/commands/blocker.rs`
- Modify: `crates/ranger-cli/src/main.rs`

**Step 1: Add subcommands for comment, tag, blocker**

**Step 2: Implement handlers**

**Step 3: Test manually**

```
cargo run --bin ranger -- comment add <task-key> "Started working on this"
cargo run --bin ranger -- comment list <task-key>
cargo run --bin ranger -- tag list
cargo run --bin ranger -- blocker add <task-key> <blocked-by-key>
```

**Step 4: Commit**

Message: `Add CLI comment, tag, and blocker commands`

---

### Task 12: End-to-end smoke test

**Files:**
- Create: `tests/integration.rs` or `crates/ranger-cli/tests/cli.rs`

**Step 1: Write an integration test exercising the full workflow**

The test should use `assert_cmd` to run the ranger binary and verify:
1. Create a backlog
2. Create tasks in it
3. List tasks (verify ordering)
4. Edit a task's state
5. Add a comment
6. Add a tag
7. Add a blocker
8. Show a task (verify all data present)
9. JSON output parses as valid JSON

Add `assert_cmd` to dev-dependencies of ranger-cli.

**Step 2: Run the integration test**

Run: `cargo test -p ranger-cli`
Expected: passes

**Step 3: Commit**

Message: `Add end-to-end integration test`

---

### Task 13: README update and cleanup

**Files:**
- Modify: `README.md`

**Step 1: Update README with installation and usage**

Add a brief "Getting Started" section showing how to build and use the CLI.

**Step 2: Run full test suite one more time**

Run: `cargo test --workspace`
Expected: all tests pass

**Step 3: Commit**

Message: `Update README with getting started instructions`
