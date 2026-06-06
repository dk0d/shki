SELECT
    n.nspname AS schema,
    c.relname AS name,
    pg_get_viewdef(c.oid, true) AS definition,
    c.relkind = 'm' AS materialized,
    a.attname AS column_name,
    format_type(a.atttypid, a.atttypmod) AS column_data_type
FROM pg_class c
JOIN pg_namespace n
    ON n.oid = c.relnamespace
LEFT JOIN pg_attribute a
    ON a.attrelid = c.oid
    AND a.attnum > 0
    AND NOT a.attisdropped
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
    AND c.relkind IN ('v', 'm')
ORDER BY n.nspname, c.relname, a.attnum
