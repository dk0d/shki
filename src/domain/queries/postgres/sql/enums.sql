SELECT
    n.nspname AS schema,
    t.typname AS name,
    array_agg(e.enumlabel ORDER BY e.enumsortorder) AS values,
    obj_description(t.oid, 'pg_type') AS description
FROM pg_type t
JOIN pg_enum e ON t.oid = e.enumtypid
JOIN pg_namespace n ON n.oid = t.typnamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
GROUP BY n.nspname, t.typname, t.oid
ORDER BY n.nspname, t.typname
