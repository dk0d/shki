SELECT
    cp.table_schema AS schema,
    cp.table_name,
    cp.column_name,
    grantee,
    privilege_type,
    is_grantable = 'YES' AS grantable
FROM information_schema.column_privileges cp
JOIN pg_namespace n
    ON n.nspname = cp.table_schema
JOIN pg_class c
    ON c.relnamespace = n.oid
    AND c.relname = cp.table_name
WHERE ($1::text IS NULL OR cp.table_schema = $1)
    AND cp.table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND cp.grantee <> pg_get_userbyid(c.relowner)
ORDER BY cp.table_schema, cp.table_name, cp.column_name, grantee, privilege_type
