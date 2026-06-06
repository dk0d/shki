SELECT
    src_ns.nspname AS table_schema,
    src_tbl.relname AS table_name,
    con.conname AS constraint_name,
    CASE con.contype
        WHEN 'p' THEN 'PRIMARY KEY'
        WHEN 'u' THEN 'UNIQUE'
        WHEN 'f' THEN 'FOREIGN KEY'
        WHEN 'c' THEN 'CHECK'
        ELSE con.contype::text
    END AS constraint_type,
    src_col.attname AS column_name,
    ref_ns.nspname AS foreign_table_schema,
    ref_tbl.relname AS foreign_table_name,
    ref_col.attname AS foreign_column_name,
    con.confupdtype::text AS update_action,
    con.confdeltype::text AS delete_action,
    con.condeferrable AS deferrable,
    con.condeferred AS initially_deferred,
    CASE
        WHEN con.contype = 'c' THEN pg_get_constraintdef(con.oid, true)
        ELSE NULL
    END AS constraint_expression
FROM pg_constraint con
JOIN pg_class src_tbl
    ON src_tbl.oid = con.conrelid
JOIN pg_namespace src_ns
    ON src_ns.oid = src_tbl.relnamespace
LEFT JOIN unnest(con.conkey) WITH ORDINALITY AS pos(attnum, ordinality)
    ON con.contype IN ('p', 'u', 'f')
LEFT JOIN pg_attribute src_col
    ON src_col.attrelid = con.conrelid
    AND src_col.attnum = pos.attnum
    AND NOT src_col.attisdropped
LEFT JOIN pg_class ref_tbl
    ON ref_tbl.oid = con.confrelid
LEFT JOIN pg_namespace ref_ns
    ON ref_ns.oid = ref_tbl.relnamespace
LEFT JOIN unnest(con.confkey) WITH ORDINALITY AS ref_pos(attnum, ordinality)
    ON con.contype = 'f'
    AND ref_pos.ordinality = pos.ordinality
LEFT JOIN pg_attribute ref_col
    ON ref_col.attrelid = con.confrelid
    AND ref_col.attnum = ref_pos.attnum
    AND NOT ref_col.attisdropped
WHERE con.contype IN ('p', 'u', 'f', 'c')
    AND ($1::text IS NULL OR src_ns.nspname = $1)
    AND src_ns.nspname NOT IN ('pg_catalog', 'information_schema')
ORDER BY src_ns.nspname, src_tbl.relname, con.conname, pos.ordinality
