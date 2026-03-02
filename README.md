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
- **Subtasks** for breaking work down
- **Blockers** for expressing dependencies
- **Tags** for grouping related work

### States

A task is always in one of four states:

| State | Meaning |
|---|---|
| **Icebox** | Captured but not committed to |
| **Backlog** | Committed and ordered by priority |
| **In Progress** | Actively being worked on |
| **Done** | Finished |

### Tags Instead of Projects

Tags replace projects. Filter any backlog by tag to see a focused slice of work. No rigid project boundaries, no duplication when a task spans concerns.

### Interface

The webapp uses an expanding modal for editing tasks — no cluttered inline editing. Design is minimalist, built with [Utopia](https://utopia.fyi/) fluid responsive CSS.

## Architecture

Ranger ships as three artifacts from one Rust codebase:

- **Library** — core data model and logic
- **CLI** — for AI agents and scripting
- **Webapp** — for human use

The CLI exists primarily so AI agents can manage tasks programmatically. The webapp exists for humans who prefer a visual interface.

## Roadmap

**First milestone:** self-host Ranger so an AI agent can use it for task management while building Ranger itself.

After that:

- Claude Code plugin for AI agent task management
- Public read-only sharing with permissions
- Configurable backlog views
- Browsing completed work
