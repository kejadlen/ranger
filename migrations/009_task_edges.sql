CREATE TABLE IF NOT EXISTS task_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    to_task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    edge_type TEXT NOT NULL CHECK (edge_type IN ('blocks', 'before')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(from_task_id, to_task_id, edge_type)
);
