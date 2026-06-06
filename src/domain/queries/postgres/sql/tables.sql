SELECT
    n.nspname AS table_schema,
    c.relname AS table_name,
    obj_description(c.oid, 'pg_class') AS table_comment,
    tblsp.spcname AS tablespace,
    COALESCE(c.reloptions, ARRAY[]::text[]) AS reloptions,
    pt.partstrat::text AS partition_strategy,
    pg_get_partkeydef(c.oid) AS partition_keydef
FROM pg_class c
JOIN pg_namespace n
    ON n.oid = c.relnamespace
LEFT JOIN pg_tablespace tblsp
    ON tblsp.oid = c.reltablespace
LEFT JOIN pg_partitioned_table pt
    ON pt.partrelid = c.oid
WHERE c.relkind IN ('r', 'p')
    AND ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY n.nspname, c.relname
