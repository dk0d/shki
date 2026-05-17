use indexmap::IndexMap;
// use indexmap::IndexMap;
use sqlx::Row;

use crate::engines::pg::Postgres;
use crate::models::table_id::TableId;
use crate::schema::{
    CheckConstraint, Column, Constraint, DataType, DbEnum, DefaultValue, ForeignKeyConstraint,
    GeneratedColumn, IdentitySpec, Index, IndexColumn, IndexMethod, NullsOrder,
    PrimaryKeyConstraint, Sequence, SequenceOptions, SortOrder, SqlDialect, UniqueConstraint,
};
use crate::snapshots::SnapshotProvider;
use crate::{Result, ShkiError};

#[derive(Clone, sqlx::FromRow)]
struct PgInfoSchemaColumnRow {
    table_schema: String,
    table_name: String,
    column_name: String,
    data_type: String,
    udt_name: String,
    is_nullable: String,
    column_default: Option<String>,
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
        let data_type = DataType::from(row.clone());
        let default = row
            .column_default
            .as_ref()
            .map(|_| DefaultValue::from(row.clone()));
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
            collation: None,
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

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<String, DbEnum>> {
        let enum_rows = if let Some(schema) = schema {
            sqlx::query(
                r#"
            SELECT
                n.nspname AS schema,
                t.typname AS name,
                array_agg(e.enumlabel ORDER BY e.enumsortorder) AS values
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = $1
            GROUP BY n.nspname, t.typname
            ORDER BY n.nspname, t.typname
            "#,
            )
            .bind(schema)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
            SELECT
                n.nspname AS schema,
                t.typname AS name,
                array_agg(e.enumlabel ORDER BY e.enumsortorder) AS values
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            GROUP BY n.nspname, t.typname
            ORDER BY n.nspname, t.typname
            "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        let mut map = IndexMap::new();
        enum_rows.into_iter().for_each(|row| {
            let schema: String = row.get("schema");
            let name: String = row.get("name");
            let values: Vec<String> = row.get("values");
            map.insert(
                name.clone(),
                DbEnum {
                    name,
                    schema: Some(schema),
                    values,
                    description: None, // TODO: introspect enum comments
                },
            );
        });

        Ok(map)
    }

    async fn get_sequences(&self, _schema: &Option<String>) -> Result<IndexMap<String, Sequence>> {
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
            cycle
        FROM pg_sequences
        WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
        ORDER BY schemaname, sequencename
        "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut map = IndexMap::new();
        for row in rows {
            map.insert(
                row.name.clone(),
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

    async fn get_tables(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, crate::schema::Table>> {
        let table_rows = if let Some(schema) = schema {
            sqlx::query(
                r#"
            SELECT
                t.table_schema,
                t.table_name,
                obj_description(c.oid, 'pg_class') AS table_comment
            FROM information_schema.tables t
            JOIN pg_namespace n ON n.nspname = t.table_schema
            JOIN pg_class c ON c.relname = t.table_name
                AND c.relnamespace = n.oid
                AND c.relkind = 'r'
            WHERE t.table_type = 'BASE TABLE'
                AND t.table_schema = $1
            ORDER BY t.table_schema, t.table_name
            "#,
            )
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(ShkiError::Database)?
        } else {
            sqlx::query(
                r#"
            SELECT
                t.table_schema,
                t.table_name,
                obj_description(c.oid, 'pg_class') AS table_comment
            FROM information_schema.tables t
            JOIN pg_namespace n ON n.nspname = t.table_schema
            JOIN pg_class c ON c.relname = t.table_name
                AND c.relnamespace = n.oid
                AND c.relkind = 'r'
            WHERE t.table_type = 'BASE TABLE'
                AND t.table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY t.table_schema, t.table_name
            "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(ShkiError::Database)?
        };

        let mut map = IndexMap::new();
        table_rows.iter().for_each(|row| {
            let schema: String = row.get("table_schema");
            let name: String = row.get("table_name");
            let comment: Option<String> = row.get("table_comment");
            map.insert(
                (name.clone(), Some(schema.clone())).into(),
                crate::schema::Table {
                    name,
                    schema: Some(schema),
                    columns: IndexMap::new(), // TODO: introspect columns
                    comment,
                    ..Default::default()
                },
            );
        });
        Ok(map)
    }

    async fn get_views(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, crate::schema::View>> {
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
    ) -> Result<IndexMap<TableId, IndexMap<String, crate::schema::Column>>> {
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
            c.is_updatable
        FROM information_schema.columns c
        WHERE ($1::text IS NULL OR c.table_schema = $1)
            AND c.table_schema NOT IN ('pg_catalog', 'information_schema')
        ORDER BY c.table_schema, c.table_name, c.ordinal_position
        "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        let mut columns_by_table: IndexMap<TableId, IndexMap<String, Column>> = IndexMap::new();

        for row in rows {
            let table_id = TableId::new(row.table_name.clone(), Some(row.table_schema.clone()));
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
    ) -> Result<IndexMap<TableId, Vec<crate::schema::Constraint>>> {
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

        let mut constraints_by_table: IndexMap<TableId, IndexMap<String, Constraint>> =
            IndexMap::new();

        for row in rows {
            let constraint_map = constraints_by_table
                .entry(TableId::new(
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
                        references_schema: row.foreign_table_schema.clone(),
                        references_table: row.foreign_table_name.clone().unwrap_or_default(),
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

                    if constraint.references_table.is_empty() {
                        constraint.references_table =
                            row.foreign_table_name.clone().ok_or_else(|| {
                                ShkiError::introspection(format!(
                                    "Foreign key '{}' is missing referenced table metadata",
                                    row.constraint_name
                                ))
                            })?;
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
    ) -> Result<IndexMap<TableId, Vec<crate::schema::Index>>> {
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
                WHEN key_col.attnum = 0 THEN pg_get_indexdef(i.indexrelid, key_col.ordinality, false)
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
        ORDER BY tbl_ns.nspname, tbl.relname, idx.relname, key_col.ordinality
            "#,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        let mut indexes_by_table: IndexMap<TableId, IndexMap<String, Index>> = IndexMap::new();

        for row in rows {
            let index_map = indexes_by_table
                .entry(TableId::new(
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

        Ok(indexes_by_table
            .into_iter()
            .map(|(table_id, indexes)| (table_id, indexes.into_values().collect()))
            .collect())
    }
}

fn parse_reference_action(action: Option<&str>) -> crate::schema::ReferenceAction {
    match action {
        Some("a") => crate::schema::ReferenceAction::NoAction,
        Some("r") => crate::schema::ReferenceAction::Restrict,
        Some("c") => crate::schema::ReferenceAction::Cascade,
        Some("n") => crate::schema::ReferenceAction::SetNull,
        Some("d") => crate::schema::ReferenceAction::SetDefault,
        _ => crate::schema::ReferenceAction::NoAction,
    }
}

fn parse_index_method(method: &str) -> IndexMethod {
    match method {
        "btree" => IndexMethod::BTree,
        "hash" => IndexMethod::Hash,
        "gist" => IndexMethod::Gist,
        "spgist" => IndexMethod::SpGist,
        "gin" => IndexMethod::Gin,
        "brin" => IndexMethod::Brin,
        _ => IndexMethod::BTree,
    }
}

fn parse_sort_order(order: Option<&str>) -> Option<SortOrder> {
    match order {
        Some("ASC") => Some(SortOrder::Asc),
        Some("DESC") => Some(SortOrder::Desc),
        _ => None,
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

fn parse_check_constraint_expression(definition: &str) -> String {
    definition
        .strip_prefix("CHECK (")
        .and_then(|expr| expr.strip_suffix(')'))
        .unwrap_or(definition)
        .to_string()
}

fn normalize_default_expression(expr: &str) -> String {
    let mut normalized = expr.trim().to_string();

    while has_wrapping_parentheses(&normalized) {
        normalized = normalized[1..normalized.len() - 1].trim().to_string();
    }

    if let Some((value, cast)) = normalized.rsplit_once("::") {
        let value = value.trim();
        let cast = cast.trim();
        if !cast.is_empty() && (is_quoted_literal(value) || is_scalar_literal(value)) {
            return value.to_string();
        }
    }

    normalized
}

fn has_wrapping_parentheses(expr: &str) -> bool {
    if !(expr.starts_with('(') && expr.ends_with(')')) {
        return false;
    }

    let inner = &expr[1..expr.len() - 1];
    let mut depth = 0_i32;
    for ch in inner.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    depth == 0
}

fn is_quoted_literal(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

fn is_scalar_literal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower == "null"
        || lower == "true"
        || lower == "false"
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
}
