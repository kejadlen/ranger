-- Backup backlog_tasks (DROP TABLE backlogs cascades the foreign key delete)
CREATE TEMPORARY TABLE backlog_tasks_backup AS SELECT * FROM backlog_tasks;

-- Rebuild backlogs without the key column, with UNIQUE on name directly
CREATE TABLE backlogs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO backlogs_new (id, name, created_at, updated_at)
SELECT id, name, created_at, updated_at FROM backlogs;

-- Drop the separate index and old table
DROP INDEX IF EXISTS idx_backlogs_name;
DROP TABLE backlogs;
ALTER TABLE backlogs_new RENAME TO backlogs;

-- Restore backlog_tasks
INSERT INTO backlog_tasks SELECT * FROM backlog_tasks_backup;
DROP TABLE backlog_tasks_backup;
