SELECT
    tbl_ns.nspname AS table_schema,
    tbl.relname AS table_name,
    idx.relname AS index_name,
    am.amname AS index_method,
    i.indisunique AS is_unique,
    (con.oid IS NOT NULL) AS is_constraint,
    pg_get_expr(i.indpred, i.indrelid) AS where_clause,
    tblsp.spcname AS tablespace,
    COALESCE(idx.reloptions, ARRAY[]::text[]) AS reloptions,
    key_col.ordinality > i.indnkeyatts AS is_include_column,
    att.attname AS column_name,
    CASE
        WHEN key_col.attnum = 0 THEN pg_get_indexdef(i.indexrelid, key_col.ordinality::int, false)
        ELSE NULL
    END AS expression,
    opc.opcname AS opclass,
    CASE
        WHEN key_col.ordinality <= i.indnkeyatts AND (i.indoption[key_col.ordinality - 1] & 1) = 1 THEN 'DESC'
        WHEN key_col.ordinality <= i.indnkeyatts THEN 'ASC'
        ELSE NULL
    END AS sort_order,
    CASE
        WHEN key_col.ordinality <= i.indnkeyatts AND (i.indoption[key_col.ordinality - 1] & 2) = 2 THEN 'FIRST'
        WHEN key_col.ordinality <= i.indnkeyatts THEN 'LAST'
        ELSE NULL
    END AS nulls_order
FROM pg_index i
JOIN pg_class idx
    ON idx.oid = i.indexrelid
JOIN pg_class tbl
    ON tbl.oid = i.indrelid
JOIN pg_namespace tbl_ns
    ON tbl_ns.oid = tbl.relnamespace
JOIN pg_am am
    ON am.oid = idx.relam
LEFT JOIN pg_tablespace tblsp
    ON tblsp.oid = idx.reltablespace
LEFT JOIN pg_constraint con
    ON con.conindid = i.indexrelid
LEFT JOIN LATERAL unnest(i.indkey::int2[]) WITH ORDINALITY AS key_col(attnum, ordinality)
    ON TRUE
LEFT JOIN LATERAL unnest(i.indclass::oid[]) WITH ORDINALITY AS key_opclass(opclass_oid, ordinality)
    ON key_opclass.ordinality = key_col.ordinality
LEFT JOIN pg_attribute att
    ON att.attrelid = i.indrelid
    AND att.attnum = key_col.attnum
    AND NOT att.attisdropped
LEFT JOIN pg_opclass opc
    ON opc.oid = key_opclass.opclass_oid
WHERE ($1::text IS NULL OR tbl_ns.nspname = $1)
    AND tbl_ns.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND NOT i.indisprimary
    AND con.oid IS NULL
ORDER BY tbl_ns.nspname, tbl.relname, idx.relname, key_col.ordinality
