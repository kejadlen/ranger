---
name: ranger
description: Use when managing tasks with the ranger CLI — creating backlogs, tracking work, picking up tasks, prioritizing, or following the ranger PM workflow in any project
---

# Ranger — Task Management

Use the `ranger` CLI to manage project tasks. Run `ranger --help` for commands and syntax.

All work must correspond to a task in the backlog. If the user asks for something that isn't tracked, create a task first, then pick it up. When the user says "let's keep working" without specifying a task, pick up the next queued task (top of the queue).

## Quick Reference

Commands use `ranger <noun> <verb>` structure. Top-level nouns: `backlog` (alias `b`), `task` (alias `t`), `comment` (alias `c`), `tag` (alias `g`).

```bash
# Backlogs
ranger backlog list                  # List all backlogs

# Tasks
ranger task create --backlog <name> "Title"   # Create a task
ranger task create --backlog <name> --state ready --description "..." "Title"
ranger task list --backlog <name>             # List tasks
ranger task show <key>                        # Show task details
ranger task edit <key> --state <state>        # Change task state
ranger task move <key> -B <other>             # Reorder: place before another task
ranger task move <key> -A <other>             # Reorder: place after another task
ranger task archive <key>                     # Archive a task (hide without deleting)
ranger task unarchive <key>                   # Restore an archived task
ranger task delete <key>                      # Delete a task entirely (rare — see Conventions)
```

```bash
# Comments
ranger comment add <task-key> "Comment text"  # Add a comment to a task
ranger comment list <task-key>                # List comments on a task
```

Tags are created automatically — there's no `tag create` command. The first `ranger tag add <task-key> <name>` makes the tag exist.

```bash
ranger tag add <task-key> <tag-name>         # Add a tag (creates it if new)
ranger tag remove <task-key> <tag-name>      # Remove a tag from a task (alias: rm)
ranger tag list                              # List tags in the current backlog
ranger tag prune                             # Remove tags no longer attached to any task
```

Task states for `--state`: `icebox`, `ready`, `in_progress`, `done`.

The `RANGER_DEFAULT_BACKLOG` env var sets the default `--backlog` value so you can omit it.

Task keys are short prefixes (e.g. `tl`) of longer IDs — use just enough to be unique. There is no `--top` or `--bottom` flag; to move to the top, use `-B` with the first task's key.

## Conventions

- **Icebox**: ideas, not committed to
- **Ready**: committed, ordered by priority (top = most important)
- **In Progress**: actively being worked on
- **Done**: the change is committed (no commit, not done)

Top of the queue = most important. Bias toward quick wins — small easy tasks should be prioritized higher by default.

### Done vs. archive vs. delete

- **Done means committed.** The transition to done happens *after* `jj commit` (or equivalent) succeeds — never before, never "I'll commit in a sec." If the change isn't in version control, the task isn't done.
- **Archive** when a task won't be done — ideas decided against, or seed tasks that have been superseded by more concrete tasks broken out of them. Archive preserves history; the task can still be referenced and unarchived later.
- **Delete** is a last resort, reserved for tasks created in error (typo duplicates, wrong backlog, accidental). Prefer archive so the trail stays intact.

## Workflow

- Move a task to **in_progress** when starting work on it (`ranger task edit <key> --state in_progress`), and back to **ready** if you stop before it's done. State should reflect what's actually happening — the board is only useful if it's accurate.
- Done = committed. Always commit first, then transition. If a task can't be tied to one or more commits, it isn't done — re-scope, split, or leave it in_progress until something lands.
- When creating or editing a task, review its tags. Add tags that apply (e.g. `bug`, area tags) and remove ones that no longer fit so `ranger task list` filters stay useful.
- Leave a comment on the task when something comes up that future-you (or the next picker-upper) would want to know but that doesn't belong in a commit message: a decision made along the way, a blocker hit, a follow-up deferred, what you tried and ruled out, where you left off when stopping mid-task, or research and feasibility findings worked out before implementation starts. Commits explain the change that landed; comments explain the trail around it — including the trail leading *up to* it. Comment bodies can be multi-line (pass a heredoc); structure and newlines are preserved.
- When archiving a task, add a comment explaining why (`ranger comment add <key> "..."`) — superseded by which tasks, decided against for what reason. The reason is the point of preserving history.
- When you encounter a bug in ranger during other work, file it in the ranger backlog (`--backlog ranger`) and tag it `bug`. Include what you observed, the expected behavior, and how to reproduce it in the description. Don't fix it inline — continue with the original task unless the bug blocks it.

---

*This is a self-improving skill — see the `self-improving-skills` skill.*
