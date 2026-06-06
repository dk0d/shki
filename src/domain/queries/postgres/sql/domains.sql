SELECT
    n.nspname AS schema,
    t.typname AS name,
    format_type(t.typbasetype, t.typtypmod) AS base_type,
    t.typnotnull AS not_null,
    t.typdefault AS default,
    obj_description(t.oid, 'pg_type') AS description
FROM pg_type t
JOIN pg_namespace n
    ON n.oid = t.typnamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND t.typtype = 'd'
ORDER BY n.nspname, t.typname
