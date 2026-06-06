SELECT
    n.nspname AS schema,
    t.typname AS type_name,
    a.attname AS column_name,
    format_type(a.atttypid, a.atttypmod) AS data_type
FROM pg_type t
JOIN pg_namespace n
    ON n.oid = t.typnamespace
JOIN pg_class c
    ON c.oid = t.typrelid
JOIN pg_attribute a
    ON a.attrelid = c.oid
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND t.typtype = 'c'
    AND c.relkind = 'c'
    AND a.attnum > 0
    AND NOT a.attisdropped
ORDER BY n.nspname, t.typname, a.attnum
