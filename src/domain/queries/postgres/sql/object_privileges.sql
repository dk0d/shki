SELECT
    table_schema AS schema,
    table_type AS object_type,
    table_name AS object_name,
    grantee,
    privilege_type,
    is_grantable = 'YES' AS grantable
FROM information_schema.table_privileges tp
JOIN information_schema.tables t
    USING (table_catalog, table_schema, table_name)
WHERE ($1::text IS NULL OR table_schema = $1)
    AND table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY table_schema, table_name, grantee, privilege_type
