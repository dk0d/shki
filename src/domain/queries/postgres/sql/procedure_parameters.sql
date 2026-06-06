SELECT
    p.oid::bigint AS function_oid,
    arg.ordinality::int AS ordinal,
    CASE COALESCE(p.proargmodes[arg.ordinality], 'i')
        WHEN 'i' THEN 'IN'
        WHEN 'o' THEN 'OUT'
        WHEN 'b' THEN 'INOUT'
        WHEN 'v' THEN 'VARIADIC'
        ELSE NULL
    END AS mode,
    p.proargnames[arg.ordinality] AS name,
    format_type(arg.type_oid, NULL) AS data_type
FROM pg_proc p
JOIN pg_namespace n
    ON n.oid = p.pronamespace
CROSS JOIN LATERAL unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[]))
    WITH ORDINALITY AS arg(type_oid, ordinality)
LEFT JOIN pg_depend dep
    ON dep.objid = p.oid
    AND dep.deptype = 'e'
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND p.prokind = 'p'
    AND dep.objid IS NULL
ORDER BY p.oid, arg.ordinality
