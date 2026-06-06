SELECT
    pn.nspname AS parent_schema,
    pc.relname AS parent_table,
    cn.nspname AS child_schema,
    cc.relname AS child_table,
    pg_get_expr(cc.relpartbound, cc.oid) AS bound
FROM pg_inherits inh
JOIN pg_class pc
    ON pc.oid = inh.inhparent
JOIN pg_namespace pn
    ON pn.oid = pc.relnamespace
JOIN pg_class cc
    ON cc.oid = inh.inhrelid
JOIN pg_namespace cn
    ON cn.oid = cc.relnamespace
WHERE ($1::text IS NULL OR pn.nspname = $1 OR cn.nspname = $1)
    AND pn.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND cn.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND EXISTS (SELECT 1 FROM pg_partitioned_table pt WHERE pt.partrelid = pc.oid)
ORDER BY pn.nspname, pc.relname, cn.nspname, cc.relname
