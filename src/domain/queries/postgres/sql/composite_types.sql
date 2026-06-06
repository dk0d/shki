SELECT
    n.nspname AS schema,
    t.typname AS name,
    obj_description(t.oid, 'pg_type') AS description
FROM pg_type t
JOIN pg_namespace n
    ON n.oid = t.typnamespace
JOIN pg_class c
    ON c.oid = t.typrelid
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND t.typtype = 'c'
    AND c.relkind = 'c'
ORDER BY n.nspname, t.typname
