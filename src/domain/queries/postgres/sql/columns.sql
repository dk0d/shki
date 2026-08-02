SELECT
    c.table_schema,
    c.table_name,
    c.column_name,
    c.data_type,
    c.udt_name,
    format_type(attr.atttypid, attr.atttypmod) AS formatted_type,
    c.is_nullable,
    c.column_default,
    c.collation_name,
    c.character_maximum_length,
    c.numeric_precision,
    c.numeric_scale,
    c.is_identity,
    c.identity_generation,
    c.identity_start,
    c.identity_increment,
    c.identity_maximum,
    c.identity_minimum,
    c.identity_cycle,
    c.is_generated,
    c.generation_expression,
    c.is_updatable,
    seq_ns.nspname AS owned_sequence_schema,
    seq_cls.relname AS owned_sequence_name,
    serial_seq.increment_by AS owned_sequence_increment,
    serial_seq.min_value AS owned_sequence_min_value,
    serial_seq.max_value AS owned_sequence_max_value,
    serial_seq.start_value AS owned_sequence_start,
    serial_seq.cache_size AS owned_sequence_cache,
    serial_seq.cycle AS owned_sequence_cycle
FROM information_schema.columns c
LEFT JOIN pg_class table_cls
    ON table_cls.relname = c.table_name
LEFT JOIN pg_namespace table_ns
    ON table_ns.oid = table_cls.relnamespace
    AND table_ns.nspname = c.table_schema
LEFT JOIN pg_attribute attr
    ON attr.attrelid = table_cls.oid
    AND attr.attname = c.column_name
    AND NOT attr.attisdropped
LEFT JOIN pg_class seq_cls
    ON seq_cls.oid = to_regclass(pg_get_serial_sequence(format('%I.%I', c.table_schema, c.table_name), c.column_name))
LEFT JOIN pg_namespace seq_ns
    ON seq_ns.oid = seq_cls.relnamespace
LEFT JOIN pg_sequences serial_seq
    ON serial_seq.schemaname = seq_ns.nspname
    AND serial_seq.sequencename = seq_cls.relname
WHERE ($1::text IS NULL OR c.table_schema = $1)
    AND c.table_schema NOT IN ('pg_catalog', 'information_schema')
    AND (table_cls.oid IS NULL OR table_ns.oid IS NOT NULL)
ORDER BY c.table_schema, c.table_name, c.ordinal_position
