SELECT
    n.nspname AS schema,
    p.proname AS name,
    p.oid::bigint AS oid,
    pg_get_function_identity_arguments(p.oid) AS identity_arguments,
    format_type(p.prorettype, NULL) AS return_type,
    format_type(a.aggtranstype, NULL) AS state_type,
    tf.proname AS transition_function_name,
    tfn.nspname AS transition_function_schema,
    ff.proname AS final_function_name,
    ffn.nspname AS final_function_schema,
    a.agginitval AS initial_condition
FROM pg_proc p
JOIN pg_namespace n
    ON n.oid = p.pronamespace
JOIN pg_aggregate a
    ON a.aggfnoid = p.oid
LEFT JOIN pg_proc tf
    ON tf.oid = a.aggtransfn
LEFT JOIN pg_namespace tfn
    ON tfn.oid = tf.pronamespace
LEFT JOIN pg_proc ff
    ON ff.oid = a.aggfinalfn
LEFT JOIN pg_namespace ffn
    ON ffn.oid = ff.pronamespace
LEFT JOIN pg_depend dep
    ON dep.objid = p.oid
    AND dep.deptype = 'e'
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg_temp_%'
    AND n.nspname NOT LIKE 'pg_toast_temp_%'
    AND p.prokind = 'a'
    AND dep.objid IS NULL
ORDER BY n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)
