use crate::engines::pg::Postgres;
use crate::models::iden::Iden;
use crate::schema::{
    CheckConstraint, Column, Constraint, DataType, DbEnum, DefaultValue, ForeignKeyConstraint,
    GeneratedColumn, IdentitySpec, Index, IndexColumn, NullsOrder, PartitionMethod, PartitionSpec,
    PrimaryKeyConstraint, Sequence, SequenceOptions, SqlDialect, Table, UniqueConstraint,
};
use crate::snapshots::SnapshotProvider;
use crate::{Result, ShkiError};
use indexmap::IndexMap;

use super::utils::{
    is_quoted_literal, is_scalar_literal, normalize_default_expression, parse_index_method,
    parse_reference_action, parse_sort_order,
};

#[derive(Clone, sqlx::FromRow)]
struct PgInfoSchemaColumnRow {
    table_schema: String,
    table_name: String,
    column_name: String,
    data_type: String,
    udt_name: String,
    is_nullable: String,
    column_default: Option<String>,
    collation_name: Option<String>,
    character_maximum_length: Option<i32>,
    numeric_precision: Option<i32>,
    numeric_scale: Option<i32>,
    is_identity: String,
    identity_generation: Option<String>,
    identity_start: Option<String>,
    identity_increment: Option<String>,
    identity_maximum: Option<String>,
    identity_minimum: Option<String>,
    identity_cycle: Option<String>,
    is_generated: String,
    generation_expression: Option<String>,
    is_updatable: String,

    // Sequence attached to this column via PG ownership metadata
    owned_sequence_schema: Option<String>,
    owned_sequence_name: Option<String>,
    owned_sequence_increment: Option<i64>,
    owned_sequence_min_value: Option<i64>,
    owned_sequence_max_value: Option<i64>,
    owned_sequence_start: Option<i64>,
    owned_sequence_cache: Option<i64>,
    owned_sequence_cycle: Option<bool>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgEnumRow {
    schema: String,
    name: String,
    values: Vec<String>,
    description: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgTableRow {
    table_schema: String,
    table_name: String,
    table_comment: Option<String>,
    tablespace: Option<String>,
    reloptions: Vec<String>,
    partition_strategy: Option<String>,
    partition_keydef: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgConstraintRow {
    table_schema: String,
    table_name: String,
    constraint_name: String,
    constraint_type: String,
    column_name: Option<String>,
    foreign_table_schema: Option<String>,
    foreign_table_name: Option<String>,
    foreign_column_name: Option<String>,
    update_action: Option<String>,
    delete_action: Option<String>,
    deferrable: bool,
    initially_deferred: bool,
    constraint_expression: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgIndexRow {
    table_schema: String,
    table_name: String,
    index_name: String,
    index_method: String,
    is_unique: bool,
    is_constraint: bool,
    where_clause: Option<String>,
    tablespace: Option<String>,
    reloptions: Vec<String>,
    is_include_column: bool,
    column_name: Option<String>,
    expression: Option<String>,
    opclass: Option<String>,
    sort_order: Option<String>,
    nulls_order: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgSequenceRow {
    schema: String,
    name: String,
    increment: i64,
    min_value: i64,
    max_value: Option<i64>,
    start: i64,
    cache: i64,
    cycle: bool,
    owned_column_type: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgViewRow {
    schema: String,
    name: String,
    definition: String,
    materialized: bool,
    column_name: Option<String>,
    column_data_type: Option<String>,
}

impl From<PgInfoSchemaColumnRow> for IdentitySpec {
    fn from(row: PgInfoSchemaColumnRow) -> Self {
        match (row.is_identity.as_str(), row.identity_generation.as_deref()) {
            ("YES", Some("ALWAYS")) => IdentitySpec {
                always: true,
                sequence_options: Some(SequenceOptions {
                    start: row.identity_start.and_then(|s| s.parse::<i64>().ok()),
                    increment: row.identity_increment.and_then(|s| s.parse::<i64>().ok()),
                    max_value: row.identity_maximum.and_then(|s| s.parse::<i64>().ok()),
                    min_value: row.identity_minimum.and_then(|s| s.parse::<i64>().ok()),
                    cycle: row.identity_cycle.as_deref() == Some("YES"),
                    cache: None, // Not exposed in information_schema
                }),
            },
            ("YES", Some("BY DEFAULT")) => IdentitySpec {
                always: false,
                sequence_options: Some(SequenceOptions {
                    start: row.identity_start.and_then(|s| s.parse::<i64>().ok()),
                    increment: row.identity_increment.and_then(|s| s.parse::<i64>().ok()),
                    max_value: row.identity_maximum.and_then(|s| s.parse::<i64>().ok()),
                    min_value: row.identity_minimum.and_then(|s| s.parse::<i64>().ok()),
                    cycle: row.identity_cycle.as_deref() == Some("YES"),
                    cache: None, // Not exposed in information_schema
                }),
            },
            _ => IdentitySpec {
                always: false,
                sequence_options: None,
            },
        }
    }
}

impl From<PgInfoSchemaColumnRow> for DefaultValue {
    fn from(row: PgInfoSchemaColumnRow) -> Self {
        if row.is_identity == "YES" {
            return DefaultValue::Identity {
                always: matches!(row.identity_generation.as_deref(), Some("ALWAYS")),
            };
        }

        let normalized = normalize_default_expression(row.column_default.as_deref().unwrap_or(""));
        let lowered = normalized.to_ascii_lowercase();

        if lowered == "null" {
            DefaultValue::Null
        } else if lowered.starts_with("nextval(") {
            DefaultValue::Sequence(normalized)
        } else if is_quoted_literal(&normalized) {
            DefaultValue::Literal(normalized[1..normalized.len() - 1].to_string())
        } else if is_scalar_literal(&normalized) {
            DefaultValue::Literal(normalized)
        } else {
            DefaultValue::Sql(normalized)
        }
    }
}

impl From<PgInfoSchemaColumnRow> for GeneratedColumn {
    fn from(row: PgInfoSchemaColumnRow) -> Self {
        GeneratedColumn {
            expression: row.generation_expression.unwrap_or_default(),
            stored: row.is_generated == "ALWAYS",
        }
    }
}

impl From<PgInfoSchemaColumnRow> for DataType {
    fn from(row: PgInfoSchemaColumnRow) -> Self {
        let PgInfoSchemaColumnRow {
            data_type,
            udt_name,
            character_maximum_length,
            numeric_precision,
            numeric_scale,
            ..
        } = row;

        let full_type = match data_type.as_str() {
            "character varying" => match character_maximum_length {
                Some(len) => format!("VARCHAR({})", len),
                None => "VARCHAR".to_string(),
            },
            "character" => match character_maximum_length {
                Some(len) => format!("CHAR({})", len),
                None => "CHAR".to_string(),
            },
            "numeric" => match (numeric_precision, numeric_scale) {
                (Some(p), Some(s)) => format!("NUMERIC({}, {})", p, s),
                (Some(p), None) => format!("NUMERIC({})", p),
                _ => "NUMERIC".to_string(),
            },
            "timestamp without time zone" => "TIMESTAMP".to_string(),
            "timestamp with time zone" => "TIMESTAMPTZ".to_string(),
            "time without time zone" => "TIME".to_string(),
            "time with time zone" => "TIMETZ".to_string(),
            "double precision" => "DOUBLE PRECISION".to_string(),
            "ARRAY" => format!("{}[]", udt_name.trim_start_matches('_').to_uppercase()),
            "USER-DEFINED" => udt_name.to_string(),
            _ => data_type.to_uppercase(),
        };

        DataType::parse(full_type, &SqlDialect::Postgres)
    }
}

impl From<PgInfoSchemaColumnRow> for Column {
    fn from(row: PgInfoSchemaColumnRow) -> Self {
        let serial_type = inline_serial_data_type(&row);
        let data_type = serial_type
            .clone()
            .unwrap_or_else(|| DataType::from(row.clone()));
        let default = if serial_type.is_some() {
            None
        } else {
            row.column_default
                .as_ref()
                .map(|_| DefaultValue::from(row.clone()))
        };
        let generated = (row.is_generated == "ALWAYS")
            .then(|| GeneratedColumn::from(row.clone()))
            .filter(|generated| !generated.expression.is_empty());
        let identity = (row.is_identity == "YES").then(|| IdentitySpec::from(row.clone()));

        Column {
            name: row.column_name,
            data_type,
            nullable: row.is_nullable == "YES",
            default,
            primary_key: false,
            unique: false,
            generated,
            comment: None,
            collation: row.collation_name,
            identity,
            references: None,
        }
    }
}

#[async_trait::async_trait]
impl SnapshotProvider for Postgres {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>> {
        let schemas: Vec<String> = if let Some(schema) = schema.as_deref() {
            // Only include the target schema if it exists
            let exists: Option<String> = sqlx::query_scalar(
                r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name = $1
            "#,
            )
            .bind(schema)
            .fetch_optional(&self.pool)
            .await?;
            exists.into_iter().collect()
        } else {
            sqlx::query_scalar(
                r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY schema_name
            "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(schemas)
    }
    async fn get_extensions(&self, _schema: &Option<String>) -> Result<Vec<String>> {
        let extensions: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT extname
            FROM pg_extension
            WHERE extname != 'plpgsql'
            ORDER BY extname
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(extensions)
    }

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>> {
        let enum_rows = if let Some(schema) = schema {
            sqlx::query_as::<_, PgEnumRow>(
                r#"
            SELECT
                n.nspname AS schema,
                t.typname AS name,
                array_agg(e.enumlabel ORDER BY e.enumsortorder) AS values,
                obj_description(t.oid, 'pg_type') AS description
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = $1
            GROUP BY n.nspname, t.typname, t.oid
            ORDER BY n.nspname, t.typname
            "#,
            )
            .bind(schema)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, PgEnumRow>(
                r#"
            SELECT
                n.nspname AS schema,
                t.typname AS name,
                array_agg(e.enumlabel ORDER BY e.enumsortorder) AS values,
                obj_description(t.oid, 'pg_type') AS description
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            GROUP BY n.nspname, t.typname, t.oid
            ORDER BY n.nspname, t.typname
            "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        let mut map = IndexMap::new();
        enum_rows.into_iter().for_each(|row| {
            map.insert(
                (row.name.clone(), Some(row.schema.clone())).into(),
                DbEnum {
                    name: row.name,
                    schema: Some(row.schema),
                    values: row.values,
                    description: row.description,
                },
            );
        });

        Ok(map)
    }

    async fn get_sequences(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>> {
        let rows = sqlx::query_as::<_, PgSequenceRow>(
            r#"
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
        WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
        ORDER BY schemaname, sequencename
        "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut map = IndexMap::new();
        for row in rows {
            if is_inline_serial_sequence(&row) {
                continue;
            }

            map.insert(
                (row.name.clone(), Some(row.schema.clone())).into(),
                Sequence {
                    name: row.name,
                    schema: Some(row.schema),
                    increment: row.increment,
                    min_value: row.min_value,
                    max_value: row.max_value,
                    start: row.start,
                    cache: row.cache,
                    cycle: row.cycle,
                },
            );
        }

        Ok(map)
    }

    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>> {
        let table_rows = if let Some(schema) = schema {
            sqlx::query_as::<_, PgTableRow>(
                r#"
            SELECT
                n.nspname AS table_schema,
                c.relname AS table_name,
                obj_description(c.oid, 'pg_class') AS table_comment,
                tblsp.spcname AS tablespace,
                COALESCE(c.reloptions, ARRAY[]::text[]) AS reloptions,
                pt.partstrat::text AS partition_strategy,
                pg_get_partkeydef(c.oid) AS partition_keydef
            FROM pg_class c
            JOIN pg_namespace n
                ON n.oid = c.relnamespace
            LEFT JOIN pg_tablespace tblsp
                ON tblsp.oid = c.reltablespace
            LEFT JOIN pg_partitioned_table pt
                ON pt.partrelid = c.oid
            WHERE c.relkind IN ('r', 'p')
                AND n.nspname = $1
            ORDER BY n.nspname, c.relname
            "#,
            )
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(ShkiError::Database)?
        } else {
            sqlx::query_as::<_, PgTableRow>(
                r#"
            SELECT
                n.nspname AS table_schema,
                c.relname AS table_name,
                obj_description(c.oid, 'pg_class') AS table_comment,
                tblsp.spcname AS tablespace,
                COALESCE(c.reloptions, ARRAY[]::text[]) AS reloptions,
                pt.partstrat::text AS partition_strategy,
                pg_get_partkeydef(c.oid) AS partition_keydef
            FROM pg_class c
            JOIN pg_namespace n
                ON n.oid = c.relnamespace
            LEFT JOIN pg_tablespace tblsp
                ON tblsp.oid = c.reltablespace
            LEFT JOIN pg_partitioned_table pt
                ON pt.partrelid = c.oid
            WHERE c.relkind IN ('r', 'p')
                AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY n.nspname, c.relname
            "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(ShkiError::Database)?
        };

        let mut map = IndexMap::new();
        table_rows.into_iter().for_each(|row| {
            map.insert(
                (row.table_name.clone(), Some(row.table_schema.clone())).into(),
                Table {
                    name: row.table_name,
                    schema: Some(row.table_schema),
                    columns: IndexMap::new(), // TODO: introspect columns
                    comment: row.table_comment,
                    options: parse_table_options(&row.reloptions),
                    tablespace: row.tablespace,
                    partition: parse_partition_spec(
                        row.partition_strategy.as_deref(),
                        row.partition_keydef.as_deref(),
                    ),
                    ..Default::default()
                },
            );
        });
        Ok(map)
    }

    async fn get_views(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, crate::schema::View>> {
        let rows = sqlx::query_as::<_, PgViewRow>(
            r#"
        SELECT
            n.nspname AS schema,
            c.relname AS name,
            pg_get_viewdef(c.oid, true) AS definition,
            c.relkind = 'm' AS materialized,
            a.attname AS column_name,
            format_type(a.atttypid, a.atttypmod) AS column_data_type
        FROM pg_class c
        JOIN pg_namespace n
            ON n.oid = c.relnamespace
        LEFT JOIN pg_attribute a
            ON a.attrelid = c.oid
            AND a.attnum > 0
            AND NOT a.attisdropped
        WHERE ($1::text IS NULL OR n.nspname = $1)
            AND n.nspname NOT IN ('pg_catalog', 'information_schema')
            AND c.relkind IN ('v', 'm')
        ORDER BY n.nspname, c.relname, a.attnum
        "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        let mut views = IndexMap::new();

        for row in rows {
            let key = (row.name.clone(), Some(row.schema.clone())).into();
            let view = views.entry(key).or_insert_with(|| crate::schema::View {
                name: row.name.clone(),
                schema: Some(row.schema.clone()),
                definition: row.definition.clone(),
                materialized: row.materialized,
                columns: Vec::new(),
            });

            if let (Some(name), Some(data_type)) = (row.column_name, row.column_data_type) {
                view.columns
                    .push(crate::schema::ViewColumn { name, data_type });
            }
        }

        Ok(views)
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Column>>> {
        let rows = sqlx::query_as::<_, PgInfoSchemaColumnRow>(
            r#"
        SELECT 
            c.table_schema,
            c.table_name,
            c.column_name,
            c.data_type,
            c.udt_name,
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
        "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        let mut columns_by_table: IndexMap<Iden, IndexMap<String, Column>> = IndexMap::new();

        for row in rows {
            let table_id = Iden::new(row.table_name.clone(), Some(row.table_schema.clone()));
            let column = Column::from(row);
            columns_by_table
                .entry(table_id)
                .or_default()
                .entry(column.name.clone())
                .insert_entry(column);
        }
        Ok(columns_by_table)
    }

    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<crate::schema::Constraint>>> {
        let rows = sqlx::query_as::<_, PgConstraintRow>(
            r#"
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
        "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        let mut constraints_by_table: IndexMap<Iden, IndexMap<String, Constraint>> =
            IndexMap::new();

        for row in rows {
            let constraint_map = constraints_by_table
                .entry(Iden::new(
                    row.table_name.clone(),
                    Some(row.table_schema.clone()),
                ))
                .or_default();

            let entry = constraint_map
                .entry(row.constraint_name.clone())
                .or_insert_with(|| match row.constraint_type.as_str() {
                    "PRIMARY KEY" => Constraint::PrimaryKey(PrimaryKeyConstraint {
                        name: Some(row.constraint_name.clone()),
                        columns: Vec::new(),
                    }),
                    "UNIQUE" => Constraint::Unique(UniqueConstraint {
                        name: Some(row.constraint_name.clone()),
                        columns: Vec::new(),
                        nulls_distinct: true,
                    }),
                    "FOREIGN KEY" => Constraint::ForeignKey(ForeignKeyConstraint {
                        name: Some(row.constraint_name.clone()),
                        columns: Vec::new(),
                        references: (
                            row.foreign_table_name.clone().unwrap_or_default(),
                            row.foreign_table_schema.clone(),
                        )
                            .into(),
                        references_columns: Vec::new(),
                        on_delete: parse_reference_action(row.delete_action.as_deref()),
                        on_update: parse_reference_action(row.update_action.as_deref()),
                        deferrable: row.deferrable,
                        initially_deferred: row.initially_deferred,
                    }),
                    _ => Constraint::Check(CheckConstraint {
                        name: Some(row.constraint_name.clone()),
                        expression: parse_check_constraint_expression(
                            row.constraint_expression.as_deref().unwrap_or_default(),
                        ),
                    }),
                });

            match entry {
                Constraint::PrimaryKey(constraint) => {
                    if let Some(column_name) = row.column_name.as_ref()
                        && !constraint.columns.contains(column_name)
                    {
                        constraint.columns.push(column_name.clone());
                    }
                }
                Constraint::Unique(constraint) => {
                    if let Some(column_name) = row.column_name.as_ref()
                        && !constraint.columns.contains(column_name)
                    {
                        constraint.columns.push(column_name.clone());
                    }
                }
                Constraint::ForeignKey(constraint) => {
                    if let Some(column_name) = row.column_name.as_ref()
                        && !constraint.columns.contains(column_name)
                    {
                        constraint.columns.push(column_name.clone());
                    }

                    if let Some(foreign_column_name) = row.foreign_column_name.as_ref()
                        && !constraint.references_columns.contains(foreign_column_name)
                    {
                        constraint
                            .references_columns
                            .push(foreign_column_name.clone());
                    }
                }
                Constraint::Check(_) | Constraint::Exclusion(_) => {}
            }
        }

        Ok(constraints_by_table
            .into_iter()
            .map(|(table_id, constraints)| (table_id, constraints.into_values().collect()))
            .collect())
    }

    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Index>>> {
        let rows = sqlx::query_as::<_, PgIndexRow>(
            r#"
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
            "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        indexes_from_rows(rows)
    }
}

fn indexes_from_rows(rows: Vec<PgIndexRow>) -> Result<IndexMap<Iden, IndexMap<String, Index>>> {
    let mut indexes_by_table: IndexMap<Iden, IndexMap<String, Index>> = IndexMap::new();

    for row in rows {
        if row.is_constraint {
            continue;
        }

        let index_map = indexes_by_table
            .entry(Iden::new(
                row.table_name.clone(),
                Some(row.table_schema.clone()),
            ))
            .or_default();

        let index = index_map
            .entry(row.index_name.clone())
            .or_insert_with(|| Index {
                name: row.index_name.clone(),
                columns: Vec::new(),
                unique: row.is_unique,
                method: parse_index_method(&row.index_method),
                where_clause: row.where_clause.clone(),
                options: parse_index_options(&row.reloptions),
                is_constraint: row.is_constraint,
                concurrently: false,
                include: Vec::new(),
                tablespace: row.tablespace.clone(),
            });

        if row.is_include_column {
            if let Some(column_name) = row.column_name.as_ref()
                && !index.include.contains(column_name)
            {
                index.include.push(column_name.clone());
            }
            continue;
        }

        let index_column = if let Some(expression) = row.expression.as_ref() {
            IndexColumn::Expression {
                expression: expression.clone(),
                order: parse_sort_order(row.sort_order.as_deref()),
                nulls: parse_nulls_order(row.nulls_order.as_deref()),
            }
        } else if let Some(column_name) = row.column_name.as_ref() {
            IndexColumn::Column {
                name: column_name.clone(),
                order: parse_sort_order(row.sort_order.as_deref()),
                nulls: parse_nulls_order(row.nulls_order.as_deref()),
                opclass: row.opclass.clone(),
            }
        } else {
            return Err(ShkiError::introspection(format!(
                "Index '{}' is missing column or expression metadata",
                row.index_name
            )));
        };

        index.columns.push(index_column);
    }

    Ok(indexes_by_table)
}

fn is_inline_serial_sequence(row: &PgSequenceRow) -> bool {
    if row.increment != 1 || row.min_value != 1 || row.start != 1 || row.cache != 1 || row.cycle {
        return false;
    }

    match row.owned_column_type.as_deref().map(str::trim) {
        Some("smallint") => row.max_value == Some(32767),
        Some("integer") => row.max_value == Some(2147483647),
        Some("bigint") => row.max_value == Some(9223372036854775807),
        _ => false,
    }
}

fn inline_serial_data_type(row: &PgInfoSchemaColumnRow) -> Option<DataType> {
    if row.is_identity == "YES" || !matches_owned_sequence_default(row) {
        return None;
    }

    if row.owned_sequence_increment != Some(1)
        || row.owned_sequence_min_value != Some(1)
        || row.owned_sequence_start != Some(1)
        || row.owned_sequence_cache != Some(1)
        || row.owned_sequence_cycle != Some(false)
    {
        return None;
    }

    match DataType::from(row.clone()) {
        DataType::SmallInt if row.owned_sequence_max_value == Some(32767) => {
            Some(DataType::SmallSerial)
        }
        DataType::Integer if row.owned_sequence_max_value == Some(2147483647) => {
            Some(DataType::Serial)
        }
        DataType::BigInt if row.owned_sequence_max_value == Some(9223372036854775807) => {
            Some(DataType::BigSerial)
        }
        _ => None,
    }
}

fn matches_owned_sequence_default(row: &PgInfoSchemaColumnRow) -> bool {
    let Some(sequence_name) = row.owned_sequence_name.as_deref() else {
        return false;
    };

    let Some(DefaultValue::Sequence(expression)) = row
        .column_default
        .as_ref()
        .map(|_| DefaultValue::from(row.clone()))
    else {
        return false;
    };

    let Some((schema, name)) = parse_regclass_name(&expression) else {
        return false;
    };

    if name != sequence_name {
        return false;
    }

    match (schema.as_deref(), row.owned_sequence_schema.as_deref()) {
        (Some(actual), Some(expected)) => actual == expected,
        (None, _) => true,
        _ => false,
    }
}

fn parse_regclass_name(expression: &str) -> Option<(Option<String>, String)> {
    let normalized = normalize_default_expression(expression);
    let inner = normalized
        .strip_prefix("nextval('")?
        .strip_suffix("'::regclass)")?;

    match inner.split_once('.') {
        Some((schema, name)) => Some((
            Some(schema.trim().trim_matches('"').to_string()),
            name.trim().trim_matches('"').to_string(),
        )),
        None => Some((None, inner.trim().trim_matches('"').to_string())),
    }
}

fn parse_nulls_order(order: Option<&str>) -> Option<NullsOrder> {
    match order {
        Some("FIRST") => Some(NullsOrder::First),
        Some("LAST") => Some(NullsOrder::Last),
        _ => None,
    }
}

fn parse_index_options(options: &[String]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|option| {
            option
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_else(|| (option.clone(), String::new()))
        })
        .collect()
}

fn parse_table_options(options: &[String]) -> IndexMap<String, String> {
    options
        .iter()
        .map(|option| {
            option
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_else(|| (option.clone(), String::new()))
        })
        .collect()
}

fn parse_partition_spec(
    strategy: Option<&str>,
    key_definition: Option<&str>,
) -> Option<PartitionSpec> {
    let method = match strategy {
        Some("r") => PartitionMethod::Range,
        Some("l") => PartitionMethod::List,
        Some("h") => PartitionMethod::Hash,
        _ => return None,
    };

    let columns = key_definition
        .map(|definition| {
            definition
                .trim()
                .strip_prefix("RANGE")
                .or_else(|| definition.trim().strip_prefix("LIST"))
                .or_else(|| definition.trim().strip_prefix("HASH"))
                .unwrap_or(definition.trim())
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(PartitionSpec { method, columns })
}

fn parse_check_constraint_expression(definition: &str) -> String {
    definition
        .strip_prefix("CHECK (")
        .and_then(|expr| expr.strip_suffix(')'))
        .unwrap_or(definition)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_options_into_index_map() {
        let options = parse_table_options(&[
            "fillfactor=90".to_string(),
            "autovacuum_enabled=false".to_string(),
        ]);

        assert_eq!(options.get("fillfactor"), Some(&"90".to_string()));
        assert_eq!(
            options.get("autovacuum_enabled"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn parses_partition_spec_from_pg_catalog_values() {
        let partition = parse_partition_spec(Some("r"), Some("RANGE (created_at, tenant_id)"))
            .expect("partition spec should be parsed");

        assert_eq!(partition.method, PartitionMethod::Range);
        assert_eq!(partition.columns, vec!["created_at", "tenant_id"]);
    }

    #[test]
    fn folds_owned_integer_sequence_into_serial_column() {
        let row = PgInfoSchemaColumnRow {
            table_schema: "public".to_string(),
            table_name: "users".to_string(),
            column_name: "id".to_string(),
            data_type: "integer".to_string(),
            udt_name: "int4".to_string(),
            is_nullable: "NO".to_string(),
            column_default: Some("nextval('users_id_seq'::regclass)".to_string()),
            collation_name: None,
            character_maximum_length: None,
            numeric_precision: None,
            numeric_scale: None,
            is_identity: "NO".to_string(),
            identity_generation: None,
            identity_start: None,
            identity_increment: None,
            identity_maximum: None,
            identity_minimum: None,
            identity_cycle: None,
            is_generated: "NEVER".to_string(),
            generation_expression: None,
            is_updatable: "YES".to_string(),
            owned_sequence_schema: Some("public".to_string()),
            owned_sequence_name: Some("users_id_seq".to_string()),
            owned_sequence_increment: Some(1),
            owned_sequence_min_value: Some(1),
            owned_sequence_max_value: Some(2147483647),
            owned_sequence_start: Some(1),
            owned_sequence_cache: Some(1),
            owned_sequence_cycle: Some(false),
        };

        let column = Column::from(row);

        assert_eq!(column.data_type, DataType::Serial);
        assert!(column.default.is_none());
    }

    #[test]
    fn keeps_custom_owned_sequence_as_regular_default() {
        let row = PgInfoSchemaColumnRow {
            table_schema: "public".to_string(),
            table_name: "users".to_string(),
            column_name: "id".to_string(),
            data_type: "integer".to_string(),
            udt_name: "int4".to_string(),
            is_nullable: "NO".to_string(),
            column_default: Some("nextval('public.users_id_seq'::regclass)".to_string()),
            collation_name: None,
            character_maximum_length: None,
            numeric_precision: None,
            numeric_scale: None,
            is_identity: "NO".to_string(),
            identity_generation: None,
            identity_start: None,
            identity_increment: None,
            identity_maximum: None,
            identity_minimum: None,
            identity_cycle: None,
            is_generated: "NEVER".to_string(),
            generation_expression: None,
            is_updatable: "YES".to_string(),
            owned_sequence_schema: Some("public".to_string()),
            owned_sequence_name: Some("users_id_seq".to_string()),
            owned_sequence_increment: Some(5),
            owned_sequence_min_value: Some(1),
            owned_sequence_max_value: Some(2147483647),
            owned_sequence_start: Some(1),
            owned_sequence_cache: Some(1),
            owned_sequence_cycle: Some(false),
        };

        let column = Column::from(row);

        assert_eq!(column.data_type, DataType::Integer);
        assert!(matches!(column.default, Some(DefaultValue::Sequence(_))));
    }

    #[test]
    fn folds_schema_qualified_owned_sequence_into_serial_column() {
        let row = PgInfoSchemaColumnRow {
            table_schema: "public".to_string(),
            table_name: "users".to_string(),
            column_name: "id".to_string(),
            data_type: "integer".to_string(),
            udt_name: "int4".to_string(),
            is_nullable: "NO".to_string(),
            column_default: Some("nextval('\"public\".\"users_id_seq\"'::regclass)".to_string()),
            collation_name: None,
            character_maximum_length: None,
            numeric_precision: None,
            numeric_scale: None,
            is_identity: "NO".to_string(),
            identity_generation: None,
            identity_start: None,
            identity_increment: None,
            identity_maximum: None,
            identity_minimum: None,
            identity_cycle: None,
            is_generated: "NEVER".to_string(),
            generation_expression: None,
            is_updatable: "YES".to_string(),
            owned_sequence_schema: Some("public".to_string()),
            owned_sequence_name: Some("users_id_seq".to_string()),
            owned_sequence_increment: Some(1),
            owned_sequence_min_value: Some(1),
            owned_sequence_max_value: Some(2147483647),
            owned_sequence_start: Some(1),
            owned_sequence_cache: Some(1),
            owned_sequence_cycle: Some(false),
        };

        let column = Column::from(row);

        assert_eq!(column.data_type, DataType::Serial);
        assert!(column.default.is_none());
    }

    #[test]
    fn keeps_owned_sequence_default_when_schema_does_not_match() {
        let row = PgInfoSchemaColumnRow {
            table_schema: "public".to_string(),
            table_name: "users".to_string(),
            column_name: "id".to_string(),
            data_type: "integer".to_string(),
            udt_name: "int4".to_string(),
            is_nullable: "NO".to_string(),
            column_default: Some("nextval('other.users_id_seq'::regclass)".to_string()),
            collation_name: None,
            character_maximum_length: None,
            numeric_precision: None,
            numeric_scale: None,
            is_identity: "NO".to_string(),
            identity_generation: None,
            identity_start: None,
            identity_increment: None,
            identity_maximum: None,
            identity_minimum: None,
            identity_cycle: None,
            is_generated: "NEVER".to_string(),
            generation_expression: None,
            is_updatable: "YES".to_string(),
            owned_sequence_schema: Some("public".to_string()),
            owned_sequence_name: Some("users_id_seq".to_string()),
            owned_sequence_increment: Some(1),
            owned_sequence_min_value: Some(1),
            owned_sequence_max_value: Some(2147483647),
            owned_sequence_start: Some(1),
            owned_sequence_cache: Some(1),
            owned_sequence_cycle: Some(false),
        };

        let column = Column::from(row);

        assert_eq!(column.data_type, DataType::Integer);
        assert!(matches!(column.default, Some(DefaultValue::Sequence(_))));
    }

    #[test]
    fn filters_out_constraint_backed_indexes_from_rows() {
        let indexes = indexes_from_rows(vec![
            PgIndexRow {
                table_schema: "public".to_string(),
                table_name: "users".to_string(),
                index_name: "users_email_key".to_string(),
                index_method: "btree".to_string(),
                is_unique: true,
                is_constraint: true,
                where_clause: None,
                tablespace: None,
                reloptions: Vec::new(),
                is_include_column: false,
                column_name: Some("email".to_string()),
                expression: None,
                opclass: Some("text_ops".to_string()),
                sort_order: Some("ASC".to_string()),
                nulls_order: Some("LAST".to_string()),
            },
            PgIndexRow {
                table_schema: "public".to_string(),
                table_name: "users".to_string(),
                index_name: "users_email_idx".to_string(),
                index_method: "btree".to_string(),
                is_unique: false,
                is_constraint: false,
                where_clause: None,
                tablespace: None,
                reloptions: Vec::new(),
                is_include_column: false,
                column_name: Some("email".to_string()),
                expression: None,
                opclass: Some("text_ops".to_string()),
                sort_order: Some("ASC".to_string()),
                nulls_order: Some("LAST".to_string()),
            },
        ])
        .expect("index rows should aggregate successfully");

        let table_indexes = indexes
            .get(&Iden::new("users", Some("public".to_string())))
            .expect("users indexes should be present");

        assert!(!table_indexes.contains_key("users_email_key"));
        assert!(table_indexes.contains_key("users_email_idx"));
    }

    #[test]
    fn does_not_treat_custom_owned_sequence_as_inline_serial_sequence() {
        let row = PgSequenceRow {
            schema: "public".to_string(),
            name: "users_id_seq".to_string(),
            increment: 5,
            min_value: 1,
            max_value: Some(2147483647),
            start: 1,
            cache: 1,
            cycle: false,
            owned_column_type: Some("integer".to_string()),
        };

        assert!(!is_inline_serial_sequence(&row));
    }

    #[test]
    fn detects_inline_serial_sequence_rows() {
        let row = PgSequenceRow {
            schema: "public".to_string(),
            name: "users_id_seq".to_string(),
            increment: 1,
            min_value: 1,
            max_value: Some(2147483647),
            start: 1,
            cache: 1,
            cycle: false,
            owned_column_type: Some("integer".to_string()),
        };

        assert!(is_inline_serial_sequence(&row));
    }
}
