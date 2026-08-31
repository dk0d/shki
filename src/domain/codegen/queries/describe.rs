//! Type a query by describing it against the compiled Shadow Database.
//!
//! Postgres' describe output tells us each parameter's type and each result
//! column's type, name, and (when it maps to a table column) its origin. We use
//! the origin to look the column up in the [`Snapshot`] — the Declarative Schema
//! is the source of truth for nullability and for resolving enum/custom types to
//! their generated Rust types. See `docs/adr/0001-typed-query-codegen.md`.

use std::collections::HashMap;

use sqlx::postgres::{PgTypeInfo, PgTypeKind};
use sqlx::{AssertSqlSafe, Column, Either, Executor, SqlSafeStr, TypeInfo};

use crate::codegen::CodegenConfig;
use crate::schema::{DataType, Table};
use crate::snapshots::Snapshot;
use crate::{Result, ShkiError};

use super::parse::{Cardinality, KeysetParam, QuerySpec};
use super::rewrite::rewrite_named_params;

/// How a parameter's value is supplied to the generated function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamBinding {
    /// A function argument. `Some(name)` for a named parameter (`$id`); `None`
    /// for a positional `$n` parameter, which the generator names `argN`.
    Arg(Option<String>),
    /// Bound from the shared `Pagination` input's `limit` (batch limit/offset).
    PageLimit,
    /// Bound from the shared `Pagination` input's `offset` (batch limit/offset).
    PageOffset,
    /// Bound from the `CursorPagination` input's key at `key_index` (batch
    /// keyset). `key_index` is the parameter's position in the `:keyset`
    /// annotation, i.e. its slot in the cursor key tuple.
    Cursor { key_index: usize, field: String },
}

/// A query parameter (`$1`, `$2`, ...) and how the generated function sources it.
#[derive(Debug, Clone)]
pub struct QueryParam {
    pub data_type: DataType,
    pub binding: ParamBinding,
    /// Marked `$name?` in the SQL: the generated argument is `Option<T>`.
    pub nullable: bool,
}

/// One column of a query's result row.
#[derive(Debug, Clone)]
pub struct ResultColumn {
    /// Output column name (becomes the row struct field name).
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

/// The shape of a query's result.
#[derive(Debug, Clone)]
pub enum QueryResult {
    /// Every column originates from a single table and covers it exactly, so the
    /// generated function reuses that table's schema struct. Carries the table's
    /// database name; the generator resolves it to the generated struct name.
    Reuse { table_name: String },
    /// A bespoke result; the generator synthesizes a row struct.
    Row { columns: Vec<ResultColumn> },
    /// `:exec` — no row type, just an affected-row count.
    Exec,
}

/// A query plus its resolved parameter and result types.
#[derive(Debug, Clone)]
pub struct DescribedQuery {
    pub spec: QuerySpec,
    /// The SQL the generated function executes: positional placeholders, with any
    /// `$name` parameters rewritten to `$n`.
    pub sql: String,
    pub params: Vec<QueryParam>,
    pub result: QueryResult,
}

/// Describe a single query against `pool` and resolve its types using `snapshot`.
pub async fn describe_query(
    pool: &sqlx::Pool<sqlx::Postgres>,
    snapshot: &Snapshot,
    config: &CodegenConfig,
    spec: QuerySpec,
) -> Result<DescribedQuery> {
    let rewritten = rewrite_named_params(&spec.sql)?;
    let is_batch = spec.cardinality == Cardinality::Batch;
    let is_keyset = is_batch && !spec.keyset.is_empty();

    // Resolve `:keyset` references to parameter indices and their cursor-key slot.
    let keyset_pos = resolve_keyset(&spec.keyset, rewritten.names.as_deref(), &spec.name)?;

    // A `:batch` query paginates by keyset (a `:keyset` annotation) or by
    // limit/offset (a `$offset` placeholder). Require one of them.
    if is_batch && !is_keyset {
        let has_offset = rewritten
            .names
            .as_ref()
            .is_some_and(|names| names.iter().any(|name| name == "offset"));
        if !has_offset {
            return Err(ShkiError::config(format!(
                "query '{}' is :batch but paginates by neither limit/offset (no $offset \
                 parameter) nor keyset (no :keyset annotation); add one",
                spec.name
            )));
        }
    }

    let stmt = AssertSqlSafe(rewritten.sql.clone()).into_sql_str();
    let described = pool.describe(stmt).await.map_err(|err| {
        ShkiError::schema(format!(
            "Failed to describe query '{}' from {}: {}",
            spec.name,
            spec.source_file.display(),
            err
        ))
    })?;

    let tables = table_index(snapshot);

    // Parameters written whole into a nullable column (INSERT VALUES / UPDATE
    // SET) become Option<T> without needing an explicit `?name`.
    let inferred_nullable = super::infer::nullable_target_params(&rewritten.sql, |table, column| {
        tables
            .get(table)
            .and_then(|table| table.columns.get(column))
            .map(|column| column.nullable)
    });

    let params: Vec<QueryParam> = match described.parameters {
        Some(Either::Left(ref type_infos)) => type_infos
            .iter()
            .enumerate()
            .map(|(idx, ti)| {
                let names = rewritten.names.as_deref();
                let binding = param_binding(names, idx, is_batch, is_keyset, &keyset_pos);
                let marked = names
                    .and_then(|names| names.get(idx))
                    .is_some_and(|name| rewritten.nullable.contains(name));
                if marked && !matches!(binding, ParamBinding::Arg(_)) {
                    return Err(ShkiError::config(format!(
                        "parameter ${} in query '{}' is marked nullable (?name), but \
                         pagination and keyset cursor parameters cannot be nullable",
                        idx + 1,
                        spec.name
                    )));
                }
                let inferred =
                    inferred_nullable.contains(&idx) && matches!(binding, ParamBinding::Arg(_));
                Ok(QueryParam {
                    data_type: pg_type_to_data_type(ti, snapshot),
                    binding,
                    nullable: marked || inferred,
                })
            })
            .collect::<Result<_>>()?,
        // `Either::Right(n)` only reports a parameter count without types. Do
        // not emit a wrapper that cannot bind its required arguments.
        Some(Either::Right(count)) => {
            return Err(ShkiError::schema(format!(
                "Failed to resolve types for {} parameter{} in query '{}' from {}",
                count,
                if count == 1 { "" } else { "s" },
                spec.name,
                spec.source_file.display(),
            )));
        }
        None => Vec::new(),
    };

    for (idx, param) in params.iter().enumerate() {
        validate_query_type(
            &param.data_type,
            snapshot,
            config,
            &spec,
            &format!("parameter ${}", idx + 1),
        )?;
    }
    for column in &described.columns {
        validate_query_type(
            &pg_type_to_data_type(column.type_info(), snapshot),
            snapshot,
            config,
            &spec,
            &format!("result column '{}'", column.name()),
        )?;
    }

    // Every keyset reference must resolve to a real parameter.
    if is_keyset {
        let bound = params
            .iter()
            .filter(|param| matches!(param.binding, ParamBinding::Cursor { .. }))
            .count();
        if bound != spec.keyset.len() {
            return Err(ShkiError::config(format!(
                "the :keyset annotation for query '{}' references a parameter the query does not have",
                spec.name
            )));
        }
    }

    let result = if spec.cardinality == Cardinality::Exec {
        QueryResult::Exec
    } else {
        resolve_result(&described, snapshot, &tables)
    };

    validate_keyset_fields(&spec, &result, snapshot)?;

    Ok(DescribedQuery {
        spec,
        sql: rewritten.sql,
        params,
        result,
    })
}

/// Query wrappers use sqlx runtime decoding, so reject types the shared schema
/// generator currently renders as `String` without a compatible sqlx decoder.
/// A configured type override is an explicit promise that the application has
/// supplied a compatible Rust type and sqlx implementation.
fn validate_query_type(
    data_type: &DataType,
    snapshot: &Snapshot,
    config: &CodegenConfig,
    spec: &QuerySpec,
    context: &str,
) -> Result<()> {
    if type_override_key(data_type).is_some_and(|key| config.type_overrides.contains_key(&key)) {
        return Ok(());
    }

    match data_type {
        DataType::Array { element_type } => {
            validate_query_type(element_type, snapshot, config, spec, context)
        }
        DataType::Numeric { .. }
        | DataType::Decimal { .. }
        | DataType::Money
        | DataType::Interval
        | DataType::Inet
        | DataType::Cidr
        | DataType::MacAddr
        | DataType::MacAddr8
        | DataType::Point
        | DataType::Line
        | DataType::LSeg
        | DataType::Box
        | DataType::Path
        | DataType::Polygon
        | DataType::Circle
        | DataType::Int4Range
        | DataType::Int8Range
        | DataType::NumRange
        | DataType::TsRange
        | DataType::TsTzRange
        | DataType::DateRange => unsupported_query_type(data_type, spec, context),
        DataType::Enum { name, schema } | DataType::Custom { name, schema }
            if !known_custom_type(name, schema.as_deref(), snapshot) =>
        {
            unsupported_query_type(data_type, spec, context)
        }
        _ => Ok(()),
    }
}

fn type_override_key(data_type: &DataType) -> Option<String> {
    match data_type {
        DataType::Enum { name, schema } | DataType::Custom { name, schema } => Some(
            schema
                .as_ref()
                .map(|schema| format!("{}.{}", schema, name))
                .unwrap_or_else(|| name.clone()),
        ),
        _ => Some(data_type.to_postgres_sql().to_lowercase()),
    }
}

fn known_custom_type(name: &str, schema: Option<&str>, snapshot: &Snapshot) -> bool {
    snapshot
        .enums()
        .keys()
        .chain(snapshot.composite_types().keys())
        .any(|iden| iden.name == name && (schema.is_none() || iden.schema.as_deref() == schema))
}

fn unsupported_query_type(data_type: &DataType, spec: &QuerySpec, context: &str) -> Result<()> {
    Err(ShkiError::config(format!(
        "query '{}' from {} has unsupported {} type '{}'; add a compatible [codegen.type_overrides] entry",
        spec.name,
        spec.source_file.display(),
        context,
        data_type.to_postgres_sql(),
    )))
}

/// Resolve `:keyset` mappings to parameter index → cursor key slot/result field.
fn resolve_keyset(
    refs: &[KeysetParam],
    names: Option<&[String]>,
    query: &str,
) -> Result<HashMap<usize, (usize, String)>> {
    let mut map = HashMap::new();
    for (key_index, reference) in refs.iter().enumerate() {
        let raw = reference.parameter.strip_prefix('$').ok_or_else(|| {
            ShkiError::config(format!(
                "keyset reference '{}' in query '{}' must start with $",
                reference.parameter, query
            ))
        })?;

        let param_index = if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
            let n: usize = raw.parse().map_err(|_| {
                ShkiError::config(format!(
                    "invalid keyset reference '{}'",
                    reference.parameter
                ))
            })?;
            if n == 0 {
                return Err(ShkiError::config(
                    "keyset reference '$0' is invalid; placeholders start at $1".to_string(),
                ));
            }
            n - 1
        } else {
            let names = names.ok_or_else(|| {
                ShkiError::config(format!(
                    "keyset reference '{}' names a parameter, but query '{}' uses positional \
                     placeholders",
                    reference.parameter, query
                ))
            })?;
            names.iter().position(|name| name == raw).ok_or_else(|| {
                ShkiError::config(format!(
                    "keyset reference '{}' does not match any parameter in query '{}'",
                    reference.parameter, query
                ))
            })?
        };
        map.insert(param_index, (key_index, reference.field.clone()));
    }
    Ok(map)
}

/// Decide how the generated function sources parameter `idx`. Keyset parameters
/// come from the `CursorPagination` cursor; in a limit/offset `:batch` query the
/// `limit`/`offset` parameters come from the shared `Pagination` input;
/// everything else is a function argument.
fn param_binding(
    names: Option<&[String]>,
    idx: usize,
    is_batch: bool,
    is_keyset: bool,
    keyset_pos: &HashMap<usize, (usize, String)>,
) -> ParamBinding {
    if let Some((key_index, field)) = keyset_pos.get(&idx) {
        return ParamBinding::Cursor {
            key_index: *key_index,
            field: field.clone(),
        };
    }
    match names.and_then(|names| names.get(idx)) {
        Some(name) if is_batch && !is_keyset && name == "limit" => ParamBinding::PageLimit,
        Some(name) if is_batch && !is_keyset && name == "offset" => ParamBinding::PageOffset,
        Some(name) => ParamBinding::Arg(Some(name.clone())),
        None => ParamBinding::Arg(None),
    }
}

fn validate_keyset_fields(
    spec: &QuerySpec,
    result: &QueryResult,
    snapshot: &Snapshot,
) -> Result<()> {
    if spec.keyset.is_empty() {
        return Ok(());
    }
    let fields: Vec<String> = match result {
        QueryResult::Row { columns } => columns.iter().map(|column| column.name.clone()).collect(),
        QueryResult::Reuse { table_name } => snapshot
            .tables()
            .into_iter()
            .find(|(iden, _)| iden.name == *table_name)
            .map(|(_, table)| table.columns.keys().cloned().collect())
            .unwrap_or_default(),
        QueryResult::Exec => Vec::new(),
    };
    for keyset in &spec.keyset {
        if !fields.contains(&keyset.field) {
            return Err(ShkiError::config(format!(
                "keyset field '{}' for query '{}' is not a selected result column",
                keyset.field, spec.name
            )));
        }
    }
    Ok(())
}

/// Build a lookup from unqualified table name to its schema definition. Column
/// origins from Postgres are unqualified, matching this key.
fn table_index(snapshot: &Snapshot) -> HashMap<String, Table> {
    snapshot
        .tables()
        .into_iter()
        .map(|(iden, table)| (iden.name, table))
        .collect()
}

fn resolve_result(
    described: &sqlx::Describe<sqlx::Postgres>,
    snapshot: &Snapshot,
    tables: &HashMap<String, Table>,
) -> QueryResult {
    let columns: Vec<ResultColumn> = described
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            // sqlx's inference: Some(true) = proven nullable in this query,
            // Some(false) = proven not null, None = unknown.
            let describe_nullable = described.nullable.get(idx).copied().flatten();

            // Prefer the Declarative Schema's view of a column that traces back to
            // a base table: it carries the authoritative nullability and resolves
            // enum/custom types to their generated Rust types.
            let from_schema = column
                .origin()
                .table_column()
                .and_then(|tc| {
                    tables
                        .get(&*tc.table)
                        .map(|table| (table, tc.name.to_string()))
                })
                .and_then(|(table, col_name)| table.columns.get(&col_name));

            let (data_type, nullable) = match from_schema {
                Some(schema_col) => (
                    schema_col.data_type.clone(),
                    // The schema's NOT NULL is authoritative unless describe
                    // proved the column nullable in this query (e.g. a NOT NULL
                    // column on the outer side of a join). "Unknown" does not
                    // override the schema.
                    schema_col.nullable || describe_nullable == Some(true),
                ),
                None => (
                    pg_type_to_data_type(column.type_info(), snapshot),
                    // No schema column to consult: nullable unless proven.
                    describe_nullable.unwrap_or(true),
                ),
            };

            // An sqlx-style alias marker overrides inference where it cannot
            // reach (e.g. UNION output columns lose their table origin).
            let (name, forced) = nullability_override(column.name());
            ResultColumn {
                name,
                data_type,
                nullable: forced.unwrap_or(nullable),
            }
        })
        .collect();

    if let Some(table_name) = reusable_table(described, tables) {
        QueryResult::Reuse { table_name }
    } else {
        QueryResult::Row { columns }
    }
}

/// Detect an sqlx-style nullability override in a column alias: `AS "id!"`
/// forces NOT NULL, `AS "note?"` forces nullable. Returns the name with the
/// marker stripped and the forced nullability, if any. Postgres' default
/// expression column name `?column?` is not an override.
fn nullability_override(name: &str) -> (String, Option<bool>) {
    let forced = if let Some(rest) = name.strip_suffix('!') {
        (!rest.is_empty()).then_some(false)
    } else if let Some(rest) = name.strip_suffix('?') {
        (!rest.is_empty() && !rest.contains('?')).then_some(true)
    } else {
        None
    };
    match forced {
        Some(_) => (name[..name.len() - 1].to_string(), forced),
        None => (name.to_string(), None),
    }
}

/// If every result column originates from the same table and together they cover
/// that table's full column set, return the table name so the generated function
/// can reuse the table's schema struct.
fn reusable_table(
    described: &sqlx::Describe<sqlx::Postgres>,
    tables: &HashMap<String, Table>,
) -> Option<String> {
    let mut table_name: Option<String> = None;
    let mut source_columns: Vec<String> = Vec::new();

    for column in &described.columns {
        let origin = column.origin();
        let tc = origin.table_column()?;
        match &table_name {
            Some(name) if name != &*tc.table => return None,
            None => table_name = Some(tc.table.to_string()),
            _ => {}
        }
        source_columns.push(tc.name.to_string());
    }

    let name = table_name?;
    let table = tables.get(&name)?;

    let mut got: Vec<&str> = source_columns.iter().map(String::as_str).collect();
    let mut want: Vec<&str> = table.columns.keys().map(String::as_str).collect();
    got.sort_unstable();
    got.dedup();
    want.sort_unstable();

    (got == want).then_some(name)
}

/// Map a Postgres type to shki's dialect-agnostic [`DataType`], resolving enums
/// known to the schema so they reach their generated Rust type.
fn pg_type_to_data_type(type_info: &PgTypeInfo, snapshot: &Snapshot) -> DataType {
    match type_info.kind() {
        PgTypeKind::Array(inner) => {
            return DataType::Array {
                element_type: Box::new(pg_type_to_data_type(inner, snapshot)),
            };
        }
        PgTypeKind::Domain(inner) => return pg_type_to_data_type(inner, snapshot),
        PgTypeKind::Enum(_) => {
            return DataType::Enum {
                name: type_info.name().to_string(),
                schema: None,
            };
        }
        _ => {}
    }

    scalar_from_name(type_info.name(), snapshot)
}

fn scalar_from_name(name: &str, snapshot: &Snapshot) -> DataType {
    match name.to_uppercase().as_str() {
        "BOOL" => DataType::Boolean,
        "INT2" => DataType::SmallInt,
        "INT4" => DataType::Integer,
        "INT8" => DataType::BigInt,
        "OID" => DataType::BigInt,
        "FLOAT4" => DataType::Real,
        "FLOAT8" => DataType::DoublePrecision,
        "NUMERIC" | "MONEY" => DataType::Numeric {
            precision: None,
            scale: None,
        },
        "TEXT" | "NAME" | "BPCHAR" | "\"CHAR\"" => DataType::Text,
        "VARCHAR" => DataType::VarChar { length: None },
        "CHAR" => DataType::Char { length: None },
        "CITEXT" => DataType::Citext,
        "UUID" => DataType::Uuid,
        "JSON" => DataType::Json,
        "JSONB" => DataType::JsonB,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time {
            precision: None,
            with_timezone: false,
        },
        "TIMETZ" => DataType::Time {
            precision: None,
            with_timezone: true,
        },
        "TIMESTAMP" => DataType::Timestamp {
            precision: None,
            with_timezone: false,
        },
        "TIMESTAMPTZ" => DataType::Timestamp {
            precision: None,
            with_timezone: true,
        },
        "INTERVAL" => DataType::Interval,
        "BYTEA" => DataType::ByteA,
        "INET" => DataType::Inet,
        "CIDR" => DataType::Cidr,
        "MACADDR" => DataType::MacAddr,
        "MACADDR8" => DataType::MacAddr8,
        other => {
            // An unrecognized name may still be a schema enum (e.g. when describe
            // reports it as a plain type). Otherwise fall back to a custom type,
            // which the type mapper renders as `String`.
            let lower = other.to_lowercase();
            if snapshot.enums().keys().any(|iden| iden.name == lower) {
                DataType::Enum {
                    name: lower,
                    schema: None,
                }
            } else {
                DataType::Custom {
                    name: lower,
                    schema: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn spec() -> QuerySpec {
        QuerySpec {
            name: "example".to_string(),
            cardinality: Cardinality::One,
            keyset: Vec::new(),
            transaction: false,
            sql: "SELECT 1".to_string(),
            source_file: PathBuf::from("queries/example.sql"),
        }
    }

    #[test]
    fn rejects_lossy_runtime_type_mappings() {
        let error = validate_query_type(
            &DataType::Numeric {
                precision: None,
                scale: None,
            },
            &Snapshot::new(crate::schema::SqlDialect::Postgres),
            &CodegenConfig::default(),
            &spec(),
            "result column 'total'",
        )
        .expect_err("numeric must not silently become String in query codegen");

        assert!(
            error
                .to_string()
                .contains("unsupported result column 'total' type 'NUMERIC'")
        );
    }

    #[test]
    fn allows_explicit_type_overrides() {
        let mut config = CodegenConfig::default();
        config
            .type_overrides
            .insert("numeric".to_string(), "bigdecimal::BigDecimal".to_string());

        validate_query_type(
            &DataType::Numeric {
                precision: None,
                scale: None,
            },
            &Snapshot::new(crate::schema::SqlDialect::Postgres),
            &config,
            &spec(),
            "parameter $1",
        )
        .expect("an explicit compatible override should be accepted");
    }

    #[test]
    fn alias_markers_override_nullability() {
        assert_eq!(nullability_override("id!"), ("id".to_string(), Some(false)));
        assert_eq!(
            nullability_override("note?"),
            ("note".to_string(), Some(true))
        );
        assert_eq!(nullability_override("plain"), ("plain".to_string(), None));
        // Postgres' default expression column name is not an override.
        assert_eq!(
            nullability_override("?column?"),
            ("?column?".to_string(), None)
        );
        // A bare marker has no name to strip to.
        assert_eq!(nullability_override("!"), ("!".to_string(), None));
        assert_eq!(nullability_override("?"), ("?".to_string(), None));
    }

    #[test]
    fn rejects_unselected_keyset_field() {
        let mut spec = spec();
        spec.keyset.push(KeysetParam {
            parameter: "$1".to_string(),
            field: "missing".to_string(),
        });
        let result = QueryResult::Row {
            columns: vec![ResultColumn {
                name: "id".to_string(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        };

        let error = validate_keyset_fields(
            &spec,
            &result,
            &Snapshot::new(crate::schema::SqlDialect::Postgres),
        )
        .expect_err("unselected keyset fields must fail generation");

        assert!(error.to_string().contains("not a selected result column"));
    }
}
