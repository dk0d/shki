SELECT
    schemaname AS schema,
    tablename AS table_name,
    policyname AS name,
    permissive = 'PERMISSIVE' AS permissive,
    roles,
    cmd AS command,
    qual AS using_expression,
    with_check AS check_expression
FROM pg_policies
WHERE ($1::text IS NULL OR schemaname = $1)
    AND schemaname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND schemaname NOT LIKE 'pg_temp_%'
    AND schemaname NOT LIKE 'pg_toast_temp_%'
ORDER BY schemaname, tablename, policyname
