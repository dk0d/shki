SELECT schema_name
FROM information_schema.schemata
WHERE ($1::text IS NULL OR schema_name = $1)
    AND schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY schema_name
