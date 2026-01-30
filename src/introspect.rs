//! Database introspection
//!
//! This module reads the current database schema and converts it to a snapshot.
//!
use indexmap::IndexMap;
use sqlx::{Pool, Postgres, Row};

use crate::Result;
use crate::schema::SchemaDialect;
use crate::snapshot::*;

/// Introspect a PostgreSQL database
pub async fn introspect_postgres(pool: &Pool<Postgres>) -> Result<Snapshot> {
    introspect_postgres_impl(pool, None).await
}

/// Introspect a specific schema in a PostgreSQL database
///
/// This function is useful for schema-isolated integration tests where you want
/// to introspect only the objects in a specific schema without interference from
/// other schemas that may exist in the database.
pub async fn introspect_postgres_schema(
    pool: &Pool<Postgres>,
    schema_name: &str,
) -> Result<Snapshot> {
    introspect_postgres_impl(pool, Some(schema_name)).await
}

async fn introspect_postgres_impl(
    pool: &Pool<Postgres>,
    target_schema: Option<&str>,
) -> Result<Snapshot> {
    let mut snapshot = Snapshot::new(SchemaDialect::Postgres);

    // Get schemas
    let schemas: Vec<String> = if let Some(schema) = target_schema {
        // Only include the target schema if it exists
        let exists: Option<String> = sqlx::query_scalar(
            r#"
            SELECT schema_name 
            FROM information_schema.schemata 
            WHERE schema_name = $1
            "#,
        )
        .bind(schema)
        .fetch_optional(pool)
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
        .fetch_all(pool)
        .await?
    };
    snapshot.schemas = schemas;

    // Get extensions (always get all extensions as they are database-wide)
    let extensions: Vec<String> = sqlx::query_scalar(
        "SELECT extname FROM pg_extension WHERE extname != 'plpgsql' ORDER BY extname",
    )
    .fetch_all(pool)
    .await?;
    snapshot.extensions = extensions;

    // Get enums
    let enum_rows = if let Some(schema) = target_schema {
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
        .fetch_all(pool)
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
        .fetch_all(pool)
        .await?
    };

    for row in enum_rows {
        let schema: String = row.get("schema");
        let name: String = row.get("name");
        let values: Vec<String> = row.get("values");

        snapshot.enums.insert(
            name.clone(),
            EnumSnapshot {
                name,
                schema: Some(schema),
                values,
                description: None, // TODO: introspect enum comments
            },
        );
    }

    // Get tables
    let table_rows = if let Some(schema) = target_schema {
        sqlx::query(
            r#"
            SELECT 
                t.table_schema,
                t.table_name
            FROM information_schema.tables t
            WHERE t.table_type = 'BASE TABLE'
                AND t.table_schema = $1
            ORDER BY t.table_schema, t.table_name
            "#,
        )
        .bind(schema)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT 
                t.table_schema,
                t.table_name
            FROM information_schema.tables t
            WHERE t.table_type = 'BASE TABLE'
                AND t.table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY t.table_schema, t.table_name
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    for row in table_rows {
        let schema: String = row.get("table_schema");
        let table_name: String = row.get("table_name");

        let columns = get_postgres_columns(pool, &schema, &table_name).await?;
        let constraints = get_postgres_constraints(pool, &schema, &table_name).await?;
        let indexes = get_postgres_indexes(pool, &schema, &table_name).await?;

        let table_snapshot = TableSnapshot {
            name: table_name.clone(),
            schema: Some(schema),
            columns,
            constraints,
            indexes,
            comment: None,
        };

        snapshot.tables.insert(table_name, table_snapshot);
    }

    Ok(snapshot)
}

async fn get_postgres_columns(
    pool: &Pool<Postgres>,
    schema: &str,
    table: &str,
) -> Result<IndexMap<String, ColumnSnapshot>> {
    let rows = sqlx::query(
        r#"
        SELECT 
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
            c.is_generated,
            c.generation_expression
        FROM information_schema.columns c
        WHERE c.table_schema = $1 AND c.table_name = $2
        ORDER BY c.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let mut columns = IndexMap::new();

    for row in rows {
        let column_name: String = row.get("column_name");
        let data_type: String = row.get("data_type");
        let udt_name: String = row.get("udt_name");
        let is_nullable: String = row.get("is_nullable");
        let column_default: Option<String> = row.get("column_default");
        let char_max_len: Option<i32> = row.get("character_maximum_length");
        let num_precision: Option<i32> = row.get("numeric_precision");
        let num_scale: Option<i32> = row.get("numeric_scale");
        let is_identity: String = row.get("is_identity");
        let identity_gen: Option<String> = row.get("identity_generation");
        let is_generated: String = row.get("is_generated");
        let gen_expr: Option<String> = row.get("generation_expression");

        // Build the full data type string
        let full_type = build_postgres_type(
            &data_type,
            &udt_name,
            char_max_len,
            num_precision,
            num_scale,
        );

        let identity = if is_identity == "YES" {
            identity_gen
        } else {
            None
        };

        let generated = if is_generated == "ALWAYS" || is_generated == "YES" {
            gen_expr.map(|e| format!("GENERATED ALWAYS AS ({}) STORED", e))
        } else {
            None
        };

        columns.insert(
            column_name.clone(),
            ColumnSnapshot {
                name: column_name,
                data_type: full_type,
                nullable: is_nullable == "YES",
                default: column_default,
                primary_key: false, // Will be set from constraints
                unique: false,      // Will be set from constraints
                generated,
                identity,
                comment: None,
                collation: None,
            },
        );
    }

    Ok(columns)
}

fn build_postgres_type(
    data_type: &str,
    udt_name: &str,
    char_max_len: Option<i32>,
    num_precision: Option<i32>,
    num_scale: Option<i32>,
) -> String {
    match data_type {
        "character varying" => match char_max_len {
            Some(len) => format!("VARCHAR({})", len),
            None => "VARCHAR".to_string(),
        },
        "character" => match char_max_len {
            Some(len) => format!("CHAR({})", len),
            None => "CHAR".to_string(),
        },
        "numeric" => match (num_precision, num_scale) {
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
    }
}

async fn get_postgres_constraints(
    pool: &Pool<Postgres>,
    schema: &str,
    table: &str,
) -> Result<Vec<ConstraintSnapshot>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            tc.constraint_name,
            tc.constraint_type,
            kcu.column_name,
            ccu.table_schema AS foreign_table_schema,
            ccu.table_name AS foreign_table_name,
            ccu.column_name AS foreign_column_name,
            rc.delete_rule,
            rc.update_rule,
            cc.check_clause
        FROM information_schema.table_constraints tc
        LEFT JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        LEFT JOIN information_schema.constraint_column_usage ccu
            ON tc.constraint_name = ccu.constraint_name
            AND tc.table_schema = ccu.table_schema
        LEFT JOIN information_schema.referential_constraints rc
            ON tc.constraint_name = rc.constraint_name
            AND tc.table_schema = rc.constraint_schema
        LEFT JOIN information_schema.check_constraints cc
            ON tc.constraint_name = cc.constraint_name
            AND tc.table_schema = cc.constraint_schema
        WHERE tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let mut constraint_map: IndexMap<String, ConstraintSnapshot> = IndexMap::new();

    for row in rows {
        let constraint_name: String = row.get("constraint_name");
        let constraint_type: String = row.get("constraint_type");
        let column_name: Option<String> = row.get("column_name");

        let entry = constraint_map
            .entry(constraint_name.clone())
            .or_insert_with(|| {
                let ctype = match constraint_type.as_str() {
                    "PRIMARY KEY" => ConstraintType::PrimaryKey,
                    "UNIQUE" => ConstraintType::Unique,
                    "FOREIGN KEY" => ConstraintType::ForeignKey,
                    "CHECK" => ConstraintType::Check,
                    _ => ConstraintType::Check,
                };

                ConstraintSnapshot {
                    name: Some(constraint_name.clone()),
                    constraint_type: ctype,
                    columns: Vec::new(),
                    references: None,
                    expression: row.get("check_clause"),
                }
            });

        if let Some(col) = column_name
            && !entry.columns.contains(&col)
        {
            entry.columns.push(col);
        }

        if constraint_type == "FOREIGN KEY" && entry.references.is_none() {
            let foreign_schema: Option<String> = row.get("foreign_table_schema");
            let foreign_table: Option<String> = row.get("foreign_table_name");
            let foreign_column: Option<String> = row.get("foreign_column_name");
            let delete_rule: Option<String> = row.get("delete_rule");
            let update_rule: Option<String> = row.get("update_rule");

            if let (Some(ft), Some(fc)) = (foreign_table, foreign_column) {
                entry.references = Some(ForeignKeyReference {
                    schema: foreign_schema,
                    table: ft,
                    columns: vec![fc],
                    on_delete: delete_rule.unwrap_or_else(|| "NO ACTION".to_string()),
                    on_update: update_rule.unwrap_or_else(|| "NO ACTION".to_string()),
                });
            }
        }
    }

    Ok(constraint_map.into_values().collect())
}

async fn get_postgres_indexes(
    pool: &Pool<Postgres>,
    schema: &str,
    table: &str,
) -> Result<IndexMap<String, IndexSnapshot>> {
    let rows = sqlx::query(
        r#"
        SELECT
            i.relname AS index_name,
            ix.indisunique AS is_unique,
            am.amname AS index_method,
            pg_get_indexdef(ix.indexrelid) AS index_def
        FROM pg_index ix
        JOIN pg_class i ON i.oid = ix.indexrelid
        JOIN pg_class t ON t.oid = ix.indrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_am am ON am.oid = i.relam
        WHERE n.nspname = $1 
            AND t.relname = $2
            AND NOT ix.indisprimary
        ORDER BY i.relname
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let mut indexes = IndexMap::new();

    for row in rows {
        let index_name: String = row.get("index_name");
        let is_unique: bool = row.get("is_unique");
        let index_method: String = row.get("index_method");
        let index_def: String = row.get("index_def");

        // Parse columns from index definition
        let columns = parse_index_columns(&index_def);
        let where_clause = parse_index_where(&index_def);

        indexes.insert(
            index_name.clone(),
            IndexSnapshot {
                name: index_name,
                columns,
                unique: is_unique,
                method: index_method,
                where_clause,
                include: Vec::new(),
            },
        );
    }

    Ok(indexes)
}

fn parse_index_columns(index_def: &str) -> Vec<String> {
    // Simple parser for index definition to extract column names
    // e.g., "CREATE INDEX idx ON table (col1, col2)"
    if let Some(start) = index_def.find('(')
        && let Some(end) = index_def.rfind(')')
    {
        let cols_str = &index_def[start + 1..end];
        return cols_str.split(',').map(|s| s.trim().to_string()).collect();
    }
    Vec::new()
}

fn parse_index_where(index_def: &str) -> Option<String> {
    index_def
        .to_lowercase()
        .find(" where ")
        .map(|idx| index_def[idx + 7..].to_string())
}
