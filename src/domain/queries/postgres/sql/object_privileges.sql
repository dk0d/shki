SELECT
    t.table_schema AS schema,
    table_type AS object_type,
    t.table_name AS object_name,
    grantee,
    privilege_type,
    is_grantable = 'YES' AS grantable
FROM information_schema.table_privileges tp
JOIN information_schema.tables t
    USING (table_catalog, table_schema, table_name)
JOIN pg_namespace n
    ON n.nspname = tp.table_schema
JOIN pg_class c
    ON c.relnamespace = n.oid
    AND c.relname = tp.table_name
WHERE ($1::text IS NULL OR t.table_schema = $1)
    AND t.table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND tp.grantee <> pg_get_userbyid(c.relowner)
ORDER BY t.table_schema, t.table_name, grantee, privilege_type
