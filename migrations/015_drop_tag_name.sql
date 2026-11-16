-- Identify tags by (key, value) and drop the stored name, which is now
-- derived: `value` for a bare tag, `key:value` otherwise.
PRAGMA foreign_keys = OFF;

-- A legacy `x:` can't be created anymore; it becomes the bare tag `x`.
UPDATE tags SET value = key, key = '' WHERE value = '' AND key <> '';

-- Tags that now share a (key, value), such as an old `bug` and `:bug`,
-- merge into the oldest one, keeping every task's links.
INSERT OR IGNORE INTO task_tags (task_id, tag_id)
SELECT tt.task_id,
       (SELECT MIN(t2.id) FROM tags t2 WHERE t2.key = t.key AND t2.value = t.value)
FROM task_tags tt
JOIN tags t ON t.id = tt.tag_id;

DELETE FROM task_tags
WHERE tag_id NOT IN (SELECT MIN(id) FROM tags GROUP BY key, value);

CREATE TABLE tags_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE (key, value)
);

INSERT INTO tags_new (id, key, value)
SELECT MIN(id), key, value FROM tags GROUP BY key, value;

-- PRAGMA foreign_keys is a no-op inside the migration's transaction, so
-- dropping `tags` cascades to `task_tags`. Back the links up first, as
-- migration 003 does for backlogs.
CREATE TEMPORARY TABLE task_tags_backup AS SELECT * FROM task_tags;

DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;

INSERT OR IGNORE INTO task_tags (task_id, tag_id)
SELECT task_id, tag_id FROM task_tags_backup;
DROP TABLE task_tags_backup;

PRAGMA foreign_keys = ON;
