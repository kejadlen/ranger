# Ranger

A personal task tracker inspired by Pivotal Tracker, built in Rust.

Ranger strips Pivotal Tracker down to its essentials: tasks, tags, and two lists. No projects, no user management, no role hierarchies — just a focused tool for tracking work.

## Why Ranger

Pivotal Tracker does too much. Most of its features — story types, epics, multi-user workflows — serve teams, not individuals. Ranger keeps what matters for solo work and discards the rest.

## Design

### Tasks

Every item is a task. No stories, bugs, chores, or features — just tasks. Each task has:

- **Title** and **description**
- **Comments** for ongoing notes
- **Tags** for grouping related work

Ranger has no subtasks and no dependency edges. Both were built and then removed as complexity that went unused, leaving a flat list ordered by priority. Break work down by creating separate tasks; record a dependency as a comment.

### States

A task is always in one of four states:

| State | Meaning |
|---|---|
| **Icebox** | Captured but not committed to |
| **Ready** | Committed and ordered by priority |
| **In Progress** | Actively being worked on |
| **Done** | Finished |

### Tags Instead of Projects

Tags replace projects. Filter any backlog by tag to see a focused slice of work. No rigid project boundaries, no duplication when a task spans concerns.

### Interface

`ranger serve` starts a web server on port 3000 that renders a backlog as a board. Tasks in the ready and icebox columns drag to reorder; everything else is read-only for now. Design is minimalist, built with [Utopia](https://utopia.fyi/) fluid responsive CSS.

## Getting Started

Build and install:

```
cargo install --path .
```

Or run directly:

```
cargo run --bin ranger -- <command>
```

### Quick start

```
ranger backlog create "my-project"
ranger task create "First thing to do" --backlog my-project
ranger task create "Second thing" --backlog my-project --state ready
ranger task list --backlog my-project
ranger tag add <key> urgent
ranger task edit <key> --state in_progress
ranger comment add <key> "Started working on this"
ranger task show <key>
```

Use `--json` on any command for machine-readable output. Backlogs are identified by name. Tasks are referenced by key prefix — type just enough characters to be unique.

Set `RANGER_DEFAULT_BACKLOG` to skip `--backlog` on every command.

The database lives at `$XDG_DATA_HOME/ranger/ranger.db` by default. Override with `--db <path>` or `RANGER_DB` env var.

### For Claude Code

This repository doubles as a Claude Code plugin marketplace. Add it with `/plugin marketplace add kejadlen/ranger`, then install the `ranger` plugin to give an agent [the skill](skills/ranger/SKILL.md) that documents this workflow.

## Architecture

Ranger is a single Rust crate with a library and binary target:

- **Library** (`ranger`) — core data model, database operations, key generation
- **CLI** (`ranger` binary) — clap-based binary for humans and AI agents
- **Webapp** (`ranger serve`) — axum server in the same binary, serving the board

The CLI exists primarily so AI agents can manage tasks programmatically. The webapp exists for humans who prefer a visual interface.

## Releases

Ranger uses a two-forge release pipeline:

1. **Gitea** (`git.kejadlen.dev`) is the source of truth. CI runs on every push to `main` (and on PRs). When CI passes, a `vYYYY-MM-DD+<short-sha>` tag is created automatically.
2. Tags mirror to **GitHub** (`github.com/kejadlen/ranger`). A tag push triggers the release workflow, which builds macOS and Linux binaries, creates a GitHub release, and publishes Dotslash configuration.

The version format is calver: the date of the commit plus its short SHA (e.g., `v2026-04-21+abc1234`).

## Roadmap

The first milestone is met: Ranger's own backlog lives in Ranger, and an agent works it while building the tool. The Claude Code plugin has shipped too.

Next:

- Configurable board columns in the web UI
- Auto-refreshing the board when data changes
- Public read-only sharing with permissions
- Browsing completed work
