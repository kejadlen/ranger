# Ranger Initial Design

**Goal:** A personal task tracker inspired by Pivotal Tracker, self-hosted so an AI agent can manage tasks via CLI while building Ranger itself.

## Stack

- **Language:** Rust
- **Database:** SQLite via sqlx (async, compile-time checked queries)
- **Async runtime:** tokio
- **CLI framework:** clap (derive)
- **DB location:** `$XDG_DATA_HOME/ranger/ranger.db` (via `dirs` or `xdg` crate), overridable with `RANGER_DB` env var

## Data Model

### Backlog

The primary organizational unit. Tasks belong to one or more backlogs.

| Column     | Type           | Notes                          |
|------------|----------------|--------------------------------|
| id         | i64            | Auto-increment, internal       |
| key        | String         | jj-style, prefix-addressable   |
| name       | String         |                                |
| created_at | DateTime\<Utc> |                                |
| updated_at | DateTime\<Utc> |                                |

### Task

Every item is a task. Subtasks are tasks with a `parent_id`.

| Column      | Type             | Notes                        |
|-------------|------------------|------------------------------|
| id          | i64              | Auto-increment, internal     |
| key         | String           | jj-style, prefix-addressable |
| parent_id   | Option\<i64>     | FK → Task, for subtasks      |
| title       | String           |                              |
| description | Option\<String>  |                              |
| state       | State            | icebox, queued, in_progress, done |
| created_at  | DateTime\<Utc>   |                              |
| updated_at  | DateTime\<Utc>   |                              |

### BacklogTask

Join table. Position is per-backlog and scoped to the task's current state.

| Column     | Type   | Notes                                    |
|------------|--------|------------------------------------------|
| backlog_id | i64    | FK → Backlog                             |
| task_id    | i64    | FK → Task                                |
| position   | String | Lexicographic fractional index           |

### Comment

| Column     | Type           | Notes          |
|------------|----------------|----------------|
| id         | i64            | Auto-increment |
| task_id    | i64            | FK → Task      |
| body       | String         |                |
| created_at | DateTime\<Utc> |                |

### Blocker

A task-to-task dependency. "This task can't proceed until that task is done."

| Column             | Type | Notes                    |
|--------------------|------|--------------------------|
| id                 | i64  | Auto-increment           |
| task_id            | i64  | FK → Task (blocked)      |
| blocked_by_task_id | i64  | FK → Task (blocking)     |

### Tag

| Column | Type   | Notes  |
|--------|--------|--------|
| id     | i64    |        |
| name   | String | Unique |

### TaskTag

| Column  | Type | Notes      |
|---------|------|------------|
| task_id | i64  | FK → Task  |
| tag_id  | i64  | FK → Tag   |

## Keys

Keys follow jj's approach: random strings generated from a pronounceable alphabet. Users reference entities by shortest unique prefix. For example, if a task has key `romoqtuw`, typing `rom` suffices if no other key shares that prefix.

## Lexicographic Positioning

Tasks are ordered within a backlog by a string-based position field. To insert between two tasks, generate a string that sorts between their positions. No renumbering required.

## CLI

### Output

Human-readable by default. `--json` flag for machine-parseable output. `RANGER_DB` env var or XDG default for database location.

### Commands

```
ranger backlog create <name>
ranger backlog list
ranger backlog show <key>

ranger task create <title> --backlog <key> [--description] [--state] [--parent] [--tag]
ranger task list [--backlog <key>] [--state <state>] [--tag <name>]
ranger task show <key>
ranger task edit <key> [--title] [--description] [--state]
ranger task move <key> [--before <key>] [--after <key>] [--backlog <key>]
ranger task add <task-key> <backlog-key>
ranger task remove <task-key> <backlog-key>
ranger task delete <key>

ranger comment add <task-key> <body>
ranger comment list <task-key>

ranger tag list

ranger blocker add <task-key> <blocked-by-key>
ranger blocker remove <task-key> <blocked-by-key>
```

## Architecture

Cargo workspace with two crates:

- **`ranger-lib`** — core data model, database operations, key generation, positioning logic. No CLI concerns.
- **`ranger-cli`** — clap-based binary that uses `ranger-lib`. Handles argument parsing, output formatting, and database path resolution.

The webapp (future) will be a third crate consuming `ranger-lib`.
