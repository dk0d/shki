SELECT
    n.nspname AS schema,
    t.typname AS domain_name,
    c.conname AS constraint_name,
    pg_get_constraintdef(c.oid, true) AS constraint_definition
FROM pg_constraint c
JOIN pg_type t
    ON t.oid = c.contypid
JOIN pg_namespace n
    ON n.oid = t.typnamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND t.typtype = 'd'
ORDER BY n.nspname, t.typname, c.conname
