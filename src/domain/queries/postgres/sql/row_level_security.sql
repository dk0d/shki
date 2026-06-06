SELECT
    n.nspname AS schema,
    c.relname AS table_name,
    c.relforcerowsecurity AS forced
FROM pg_class c
JOIN pg_namespace n
    ON n.oid = c.relnamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND c.relkind IN ('r', 'p')
    AND c.relrowsecurity
ORDER BY n.nspname, c.relname
