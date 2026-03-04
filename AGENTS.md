# Ranger — Agent Instructions

## Project

Personal task tracker. Rust workspace: `ranger-lib` (library) + `ranger-cli` (binary). SQLite via sqlx, async with tokio, CLI with clap.

## Commands

```bash
cargo build --workspace          # Build everything
cargo test --workspace           # Run all tests (33 unit + 1 integration)
cargo test -p ranger-lib         # Library tests only
cargo test -p ranger-cli         # CLI integration test only
cargo run --bin ranger -- --help  # CLI usage
```

## Project Management

Use the `ranger` CLI to manage tasks for this project. The database lives at the default XDG path (`~/.local/share/ranger/ranger.db`).

### Setup (first time only)

```bash
cargo run --bin ranger -- backlog create "Ranger"
```

### Workflow

Before starting work, check the backlog:

```bash
ranger task list --backlog <key> --state queued
ranger task list --backlog <key> --state in_progress
```

When picking up a task:

```bash
ranger task edit <key> --state in_progress
ranger comment add <key> "Starting work on this"
```

While working, add comments to track progress and decisions:

```bash
ranger comment add <key> "Decided to use X because Y"
```

When done:

```bash
ranger task edit <key> --state done
ranger comment add <key> "Completed — summary of what was done"
```

To add new work:

```bash
ranger task create "Title" --backlog <key>                    # icebox by default
ranger task create "Title" --backlog <key> --state queued     # committed work
ranger task create "Subtask" --backlog <key> --parent <key>   # subtask
```

### Prioritization

When adding queued tasks, consider where they belong relative to existing work. New tasks land at the bottom by default — reposition them if they're higher priority:

```bash
ranger task list --backlog <key> --state queued               # see current order
ranger task move <key> --backlog <key> --before <key>         # place before a task
ranger task move <key> --backlog <key> --after <key>          # place after a task
```

Top of the queue = most important. Ask the user where a task should go if priority isn't obvious. Don't just append everything to the bottom — a backlog that isn't ordered isn't useful.

Use `--json` on any command when you need structured output.

### Working in the Open

Always use the `working-in-the-open` skill when working on ranger tasks. Use `ranger comment add` to post updates instead of GitHub issue comments.

### Conventions

- **Icebox**: ideas, not committed to
- **Queued**: committed, ordered by priority (top = most important)
- **In Progress**: actively being worked on
- **Done**: finished

## Architecture

```
crates/
├── ranger-lib/          # Core library
│   ├── src/
│   │   ├── db.rs        # SQLite connection, migrations
│   │   ├── error.rs     # Error types
│   │   ├── key.rs       # jj-style key generation
│   │   ├── models.rs    # Data types (Backlog, Task, Comment, Tag, Blocker)
│   │   ├── position.rs  # Lexicographic fractional indexing
│   │   └── ops/         # CRUD operations per model
│   └── migrations/      # SQL schema
└── ranger-cli/          # CLI binary
    ├── src/
    │   ├── main.rs      # Entrypoint, clap setup, DB path resolution
    │   ├── output.rs    # Human/JSON output helpers
    │   └── commands/    # One module per subcommand group
    └── tests/
        └── cli.rs       # End-to-end integration test
```

## Key Design Decisions

- **Keys**: jj-style random strings (16 chars, `k-z` alphabet). Reference by shortest unique prefix.
- **Positioning**: Lexicographic string-based ordering within backlogs. Insert between two positions without renumbering.
- **Tasks in multiple backlogs**: A task can belong to multiple backlogs via `backlog_tasks` join table, with independent positions.
- **Subtasks are tasks**: `parent_id` on tasks — subtasks get full task capabilities.
- **No compile-time checked queries**: Using `sqlx::query_as` with runtime binding, not `query_as!` macros. No need for `DATABASE_URL` at build time.
- **Dependencies unpinned**: `Cargo.toml` uses `"*"` versions; `Cargo.lock` pins exact versions.

## Testing

Tests use `tempfile` for isolated SQLite databases. Each test creates its own DB — no shared state.

The integration test (`crates/ranger-cli/tests/cli.rs`) exercises the full workflow via the compiled binary using `assert_cmd`.

## Gotchas

- `sqlx::raw_sql` is used for migrations (multiple statements in one file). `sqlx::query` only runs one statement.
- SQLite foreign keys must be enabled per-connection (`foreign_keys(true)` on connect options).
- The `xdg` crate resolves `$XDG_DATA_HOME/ranger/ranger.db`. Override with `RANGER_DB` env var or `--db` flag.
- Migration uses `CREATE TABLE IF NOT EXISTS` so it's idempotent (safe to run on every connect).

## VCS

This project uses **jj** (Jujutsu), not git directly. Use `jj` commands for commits, diffs, and history.
