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

Use the `ranger` CLI to manage tasks for this project. The database lives at the default XDG path (`~/.local/share/ranger/ranger.db`).

### Setup (first time only)

```bash
ranger backlog create "ranger"
```

> **Note:** Always use the installed `ranger` binary for PM tasks, not `cargo run`. The repo may be in a non-compiling state during development. Install with `cargo install --path . --locked`.

Set `RANGER_DEFAULT_BACKLOG=ranger` to skip `--backlog` on every command.

### Workflow

All work must correspond to a task in the backlog. If the user asks for something that isn't tracked, create a task for it first, then pick it up.

Before starting work, check the backlog:

```bash
ranger backlog show <name>
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

When done, commit first, then mark the task:

```bash
jj commit -m "description of the change"
ranger task edit <key> --state done
ranger comment add <key> "Completed — summary of what was done"
```

To add new work:

```bash
ranger task create "Title" --backlog <name>                    # icebox by default
ranger task create "Title" --backlog <name> --state queued     # committed work
ranger task create "Subtask" --backlog <name> --parent <key>   # subtask
```

### Prioritization

When adding queued tasks, consider where they belong relative to existing work. New tasks land at the bottom by default — reposition them if they're higher priority:

```bash
ranger backlog show <name>                                     # see current order
ranger task move <key> --backlog <name> --before <key>         # place before a task
ranger task move <key> --backlog <name> --after <key>          # place after a task
```

Top of the queue = most important. Only move tasks within the queued state — don't reposition done, in_progress, or icebox tasks. Ask the user where a task should go if priority isn't obvious. Don't just append everything to the bottom — a backlog that isn't ordered isn't useful.

**Bias toward quick wins**: Small, easy tasks should be prioritized higher by default — even if they're just nice-to-haves. A 5-minute fix that improves quality of life is worth doing before a multi-hour feature. When suggesting priority, bump quick wins up rather than defaulting them to the bottom.

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
- **Tasks in multiple backlogs**: A task can belong to multiple backlogs via `backlog_tasks` join table, with independent positions.
- **Subtasks are tasks**: `parent_id` on tasks — subtasks get full task capabilities.
- **No compile-time checked queries**: Using `sqlx::query_as` with runtime binding, not `query_as!` macros. No need for `DATABASE_URL` at build time.

## Testing

Tests use `tempfile` for isolated SQLite databases. Each test creates its own DB — no shared state.

The integration test (`tests/cli.rs`) exercises the full workflow via the compiled binary using `assert_cmd`.

## Gotchas

- `sqlx::raw_sql` is used for migrations (multiple statements in one file). `sqlx::query` only runs one statement.
- SQLite foreign keys must be enabled per-connection (`foreign_keys(true)` on connect options).
- The `xdg` crate resolves `$XDG_DATA_HOME/ranger/ranger.db`. Override with `RANGER_DB` env var or `--db` flag.
- Backlogs are identified by name, not key. `RANGER_DEFAULT_BACKLOG` sets the default for `--backlog` flags.
- Migration uses `CREATE TABLE IF NOT EXISTS` so it's idempotent (safe to run on every connect).

## VCS

This project uses **jj** (Jujutsu), not git directly. Use `jj` commands for commits, diffs, and history.

Use jj workspaces (in `work/`) for feature work. See the `jj-workspaces` skill.
