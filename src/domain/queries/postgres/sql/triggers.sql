SELECT
    n.nspname AS schema,
    c.relname AS table_name,
    t.tgname AS name,
    p.proname AS function_name,
    pn.nspname AS function_schema,
    t.tgtype::int AS trigger_type
FROM pg_trigger t
JOIN pg_class c
    ON c.oid = t.tgrelid
JOIN pg_namespace n
    ON n.oid = c.relnamespace
JOIN pg_proc p
    ON p.oid = t.tgfoid
JOIN pg_namespace pn
    ON pn.oid = p.pronamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND NOT t.tgisinternal
ORDER BY n.nspname, c.relname, t.tgname
