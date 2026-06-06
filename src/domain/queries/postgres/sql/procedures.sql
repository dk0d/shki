SELECT
    n.nspname AS schema,
    p.proname AS name,
    p.oid::bigint AS oid,
    pg_get_function_identity_arguments(p.oid) AS identity_arguments,
    l.lanname AS language,
    COALESCE(pg_get_function_sqlbody(p.oid), p.prosrc) AS body
FROM pg_proc p
JOIN pg_namespace n
    ON n.oid = p.pronamespace
JOIN pg_language l
    ON l.oid = p.prolang
LEFT JOIN pg_depend dep
    ON dep.objid = p.oid
    AND dep.deptype = 'e'
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND p.prokind = 'p'
    AND dep.objid IS NULL
ORDER BY n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)
