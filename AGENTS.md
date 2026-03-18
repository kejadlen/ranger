# Ranger — Agent Instructions

## Project

Personal task tracker. Single Rust crate with a library and binary target. SQLite via sqlx, async with tokio, CLI with clap.

## Commands

```bash
just fmt                          # Format all code
just check                        # Type-check
just clippy                       # Lint (deny warnings)
just coverage                     # Run tests with coverage (fail under 100%)
just all                          # fmt + clippy + coverage
ranger --help                     # CLI usage (installed binary)
```

## Project Management

Use the `ranger` skill for task management workflow. The backlog for this project is `ranger`. Set `RANGER_DEFAULT_BACKLOG=ranger` to skip `--backlog` on every command.

> **Note:** Always use the installed `ranger` binary for PM tasks, not `cargo run`. The repo may be in a non-compiling state during development. Install with `cargo install --path . --locked`.

**Use a jj workspace** (see `jj-workspaces` skill) for all feature work unless the change is exceedingly simple (e.g., a one-line config tweak or AGENTS.md update). Name workspaces `<key-prefix>-<short-descriptor>` (e.g., `voxv-position-args`).

## Architecture

```
src/
├── lib.rs               # Library root
├── db.rs                # SQLite connection, migrations
├── error.rs             # Error types
├── key.rs               # jj-style key generation
├── models.rs            # Data types (Backlog, Task, Comment, Tag, Blocker)
├── position.rs          # Lexicographic fractional indexing
├── ops/                 # CRUD operations per model
└── bin/ranger/          # CLI binary
    ├── main.rs          # Entrypoint, clap setup, DB path resolution
    ├── output.rs        # Human/JSON output helpers
    └── commands/        # One module per subcommand group
migrations/              # SQL schema
tests/
└── cli.rs               # End-to-end integration test
```

## Key Design Decisions

- **Keys**: jj-style random strings (16 chars, `k-z` alphabet). Reference by shortest unique prefix.
- **Positioning**: Lexicographic string-based ordering within backlogs. Insert between two positions without renumbering.
- **Single crate**: Library (`src/lib.rs`) and binary (`src/bin/ranger/`) in one crate. No workspace.
- **Single backlog per task**: Each task belongs to exactly one backlog. `backlog_id` and `position` live directly on the `tasks` table.
- **Subtasks are tasks**: `parent_id` on tasks — subtasks get full task capabilities.
- **Tags**: Free-form labels on tasks via a many-to-many join table (`task_tags`). Used for cross-cutting concerns like `web`, `cli`, `infra`. Filter tasks by tag with `--tag`.
- **No compile-time checked queries**: Using `sqlx::query_as` with runtime binding, not `query_as!` macros. No need for `DATABASE_URL` at build time.
- **Web UI browser targets**: Latest Firefox and Safari. Modern APIs (Popover, CSS anchor positioning) are fair game.

## Testing

Tests use `tempfile` for isolated SQLite databases. Each test creates its own DB — no shared state.

The integration test (`tests/cli.rs`) exercises the full workflow via the compiled binary using `assert_cmd`.

## Environment

When installing system packages (`apt-get`), adding Rust components (`rustup component add`), or making any other system-level change, ask whether `.ramekin/Dockerfile` should be updated so the change persists across sessions.

## Gotchas

- `sqlx::raw_sql` is used for migrations (multiple statements in one file). `sqlx::query` only runs one statement.
- SQLite foreign keys must be enabled per-connection (`foreign_keys(true)` on connect options).
- The `xdg` crate resolves `$XDG_DATA_HOME/ranger/ranger.db`. Override with `RANGER_DB` env var or `--db` flag.
- Backlogs are identified by name, not key. `RANGER_DEFAULT_BACKLOG` sets the default for `--backlog` flags.
- Migration uses `CREATE TABLE IF NOT EXISTS` so it's idempotent (safe to run on every connect).
- **Never modify existing migrations.** They have already been run against real databases. Schema changes go in new migration files only.
- **Migrations must not lose data.** When recreating a table, always `INSERT INTO ... SELECT` all rows from the original, including data in join tables (e.g. `task_tags`). Test migrations against a database with real data, not just empty schemas.
- SQLite doesn't support `ALTER TABLE DROP COLUMN` with foreign keys cleanly. When recreating a table, wrap in `PRAGMA foreign_keys = OFF/ON` to prevent `ON DELETE CASCADE` from wiping join tables (e.g. `task_tags`).

## VCS

This project uses **jj** (Jujutsu), not git directly. Use `jj` commands for commits, diffs, and history.

**Prefer jj workspaces** (in `work/`) for feature work. See the `jj-workspaces` skill. Repo-wide changes like AGENTS.md updates should be made in the main workspace, not in feature workspaces.

### Finishing Workspace Work

When work in a workspace is complete and verified:

```bash
# 1. Return to main workspace
cd /Users/alpha/src/ranger

# 2. Sync to see workspace changes
jj workspace update-stale

# 3. Rebase workspace commits after the last described commit on main's line
jj rebase -s <first-workspace-commit> -A 'latest(trunk()..default@ ~ description(exact:""))'

# 4. Clean up — forget drops the workspace, then abandon its leftover empty WC
jj workspace forget <name>
jj abandon 'empty() & description(exact:"") & @-'
rm -rf work/<name>
```

The revset `latest(trunk()..default@ ~ description(exact:""))` finds the most recent commit after trunk that has a non-empty description — i.e., the last explicitly committed change on the main line.
