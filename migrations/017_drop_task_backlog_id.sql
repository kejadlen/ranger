-- Tasks no longer belong to a backlog; their `project:` tag (migration 016)
-- says which project they're in. Positions become one global order.
--
-- Positions were only ordered within a backlog, so each one is prefixed with
-- two letters encoding the backlog's rank by id. That keeps every backlog's
-- tasks together and in their existing order. Ranks map to `bb`..`yy`, which
-- leaves room before and after, and the old position never ends in `a`, so
-- the result is still a valid position.
PRAGMA foreign_keys = OFF;

CREATE TABLE tasks_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL DEFAULT 'icebox'
        CHECK (state IN ('icebox', 'ready', 'in_progress', 'done')),
    position TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    done_at TEXT,
    CHECK ((state = 'done') = (done_at IS NOT NULL))
);

INSERT INTO tasks_new (id, key, title, description, state, position, archived, created_at, updated_at, done_at)
SELECT t.id, t.key, t.title, t.description, t.state,
       char(98 + r.rank / 24, 98 + r.rank % 24) || t.position,
       t.archived, t.created_at, t.updated_at, t.done_at
FROM tasks t
JOIN (SELECT id, (SELECT COUNT(*) FROM backlogs b2 WHERE b2.id < b.id) AS rank
      FROM backlogs b) r ON r.id = t.backlog_id;

-- PRAGMA foreign_keys is a no-op inside the migration's transaction, so
-- dropping `tasks` cascades to its children. Back them up first, as
-- migrations 003 and 015 do.
CREATE TEMPORARY TABLE task_tags_backup AS SELECT * FROM task_tags;
CREATE TEMPORARY TABLE comments_backup AS SELECT * FROM comments;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

INSERT OR IGNORE INTO task_tags (task_id, tag_id)
SELECT task_id, tag_id FROM task_tags_backup;
INSERT OR IGNORE INTO comments (id, task_id, body, created_at)
SELECT id, task_id, body, created_at FROM comments_backup;
DROP TABLE task_tags_backup;
DROP TABLE comments_backup;

PRAGMA foreign_keys = ON;
