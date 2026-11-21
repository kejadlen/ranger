-- Backlogs are now only `project:` tags (migrations 016 and 017), so the
-- table has nothing left pointing at it. Each backlog's tag stays, even for
-- an empty backlog, until `tag prune` removes it.
DROP TABLE backlogs;
