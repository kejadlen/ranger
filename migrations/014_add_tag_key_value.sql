-- Store each tag's `key:value` parse alongside its name, splitting on the
-- first colon. A bare tag such as `bug` has an empty key, the same as `:bug`.
--
-- New tags must have a non-empty value, but existing ones are split as they
-- are rather than dropped, so a legacy `x:` keeps an empty value.
ALTER TABLE tags ADD COLUMN key TEXT NOT NULL DEFAULT '';
ALTER TABLE tags ADD COLUMN value TEXT NOT NULL DEFAULT '';

UPDATE tags
SET key = CASE WHEN instr(name, ':') > 0
               THEN substr(name, 1, instr(name, ':') - 1)
               ELSE '' END,
    value = CASE WHEN instr(name, ':') > 0
                 THEN substr(name, instr(name, ':') + 1)
                 ELSE name END;
