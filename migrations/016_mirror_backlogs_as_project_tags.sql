-- Mirror each backlog into a `project:<name>` tag and attach it to every
-- task in that backlog. From here on, creating a task writes both.
INSERT OR IGNORE INTO tags (key, value)
SELECT 'project', name FROM backlogs;

INSERT OR IGNORE INTO task_tags (task_id, tag_id)
SELECT t.id, tg.id
FROM tasks t
JOIN backlogs b ON b.id = t.backlog_id
JOIN tags tg ON tg.key = 'project' AND tg.value = b.name;
