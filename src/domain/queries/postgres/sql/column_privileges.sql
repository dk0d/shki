SELECT
    table_schema AS schema,
    table_name,
    column_name,
    grantee,
    privilege_type,
    is_grantable = 'YES' AS grantable
FROM information_schema.column_privileges
WHERE ($1::text IS NULL OR table_schema = $1)
    AND table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY table_schema, table_name, column_name, grantee, privilege_type
