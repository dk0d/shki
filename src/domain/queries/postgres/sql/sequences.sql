SELECT
    schemaname AS schema,
    sequencename AS name,
    increment_by AS increment,
    min_value,
    max_value,
    start_value AS start,
    cache_size AS cache,
    cycle,
    format_type(attr.atttypid, attr.atttypmod) AS owned_column_type
FROM pg_sequences seq
LEFT JOIN pg_class seq_cls
    ON seq_cls.relname = seq.sequencename
LEFT JOIN pg_namespace seq_ns
    ON seq_ns.oid = seq_cls.relnamespace
    AND seq_ns.nspname = seq.schemaname
LEFT JOIN pg_depend dep
    ON dep.objid = seq_cls.oid
    AND dep.classid = 'pg_class'::regclass
    AND dep.refclassid = 'pg_class'::regclass
    AND dep.deptype = 'a'
LEFT JOIN pg_class table_cls
    ON table_cls.oid = dep.refobjid
LEFT JOIN pg_attribute attr
    ON attr.attrelid = table_cls.oid
    AND attr.attnum = dep.refobjsubid
WHERE ($1::text IS NULL OR schemaname = $1)
    AND schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY schemaname, sequencename
