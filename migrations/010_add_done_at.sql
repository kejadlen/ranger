-- Add done_at timestamp. Non-null iff state = 'done'.
-- Requires table recreation to backfill existing done tasks atomically
-- with the CHECK constraint.

PRAGMA foreign_keys = OFF;

CREATE TABLE tasks_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    backlog_id INTEGER NOT NULL REFERENCES backlogs(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL DEFAULT 'icebox',
    position TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    done_at TEXT,
    CHECK ((state = 'done') = (done_at IS NOT NULL))
);

INSERT INTO tasks_new (id, key, backlog_id, title, description, state, position, archived, created_at, updated_at, done_at)
SELECT id, key, backlog_id, title, description, state, position, archived, created_at, updated_at,
    CASE WHEN state = 'done' THEN updated_at ELSE NULL END
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

PRAGMA foreign_keys = ON;
