-- Store each tag's `key:value` parse alongside its name. A tag splits on its
-- first colon; bare tags such as `bug` keep NULL in both columns.
ALTER TABLE tags ADD COLUMN key TEXT;
ALTER TABLE tags ADD COLUMN value TEXT;

UPDATE tags
SET key = substr(name, 1, instr(name, ':') - 1),
    value = substr(name, instr(name, ':') + 1)
WHERE instr(name, ':') > 0;
