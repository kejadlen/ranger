-- Move from many-to-many (backlog_tasks) to direct ownership (tasks.backlog_id + tasks.position).
-- For tasks in multiple backlogs, keep the first (lowest backlog_id) association.

ALTER TABLE tasks ADD COLUMN backlog_id INTEGER REFERENCES backlogs(id) ON DELETE CASCADE;
ALTER TABLE tasks ADD COLUMN position TEXT NOT NULL DEFAULT '';

-- Populate from backlog_tasks — pick the lowest backlog_id per task
UPDATE tasks SET
    backlog_id = (
        SELECT bt.backlog_id FROM backlog_tasks bt
        WHERE bt.task_id = tasks.id
        ORDER BY bt.backlog_id ASC LIMIT 1
    ),
    position = (
        SELECT bt.position FROM backlog_tasks bt
        WHERE bt.task_id = tasks.id
        ORDER BY bt.backlog_id ASC LIMIT 1
    );

-- Drop orphaned tasks (not in any backlog) — shouldn't happen, but be safe
DELETE FROM tasks WHERE backlog_id IS NULL;

-- Drop the join table
DROP TABLE backlog_tasks;
