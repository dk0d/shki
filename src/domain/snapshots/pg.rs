use crate::engines::pg::Postgres;
use crate::models::iden::Iden;
use crate::queries::postgres::snapshot as pg_snapshot_queries;
use crate::schema::{
    Aggregate, CheckConstraint, Column, ColumnPrivilege, CompositeType, CompositeTypeColumn,
    Constraint, DataType, DbEnum, DefaultPrivilege, DefaultValue, Domain, DomainConstraint,
    ForeignKeyConstraint, Function, FunctionParameter, FunctionParameterMode, GeneratedColumn,
    IdentitySpec, Index, IndexColumn, IndexMethod, NullsOrder, ObjectPrivilege,
    PartitionAttachment, PartitionMethod, PartitionSpec, PrimaryKeyConstraint, Procedure,
    RevokedDefaultPrivilege, RowLevelSecurity, RowLevelSecurityPolicy, Sequence, SequenceOptions,
    SortOrder, SqlDialect, Table, Trigger, TriggerEvent, TriggerOrientation, TriggerTiming,
    UniqueConstraint,
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
    formatted_type: Option<String>,
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
struct PgCompositeTypeRow {
    schema: String,
    name: String,
    description: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgCompositeTypeColumnRow {
    schema: String,
    type_name: String,
    column_name: String,
    data_type: String,
}

#[derive(Clone, sqlx::FromRow)]
struct PgDomainRow {
    schema: String,
    name: String,
    base_type: String,
    not_null: bool,
    default: Option<String>,
    description: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgDomainConstraintRow {
    schema: String,
    domain_name: String,
    constraint_name: String,
    constraint_definition: String,
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

#[derive(Clone, sqlx::FromRow)]
struct PgFunctionRow {
    schema: String,
    name: String,
    oid: i64,
    identity_arguments: String,
    return_type: Option<String>,
    language: String,
    body: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgProcedureRow {
    schema: String,
    name: String,
    oid: i64,
    identity_arguments: String,
    language: String,
    body: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgAggregateRow {
    schema: String,
    name: String,
    oid: i64,
    identity_arguments: String,
    return_type: String,
    state_type: String,
    transition_function_name: Option<String>,
    transition_function_schema: Option<String>,
    final_function_name: Option<String>,
    final_function_schema: Option<String>,
    initial_condition: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgFunctionParameterRow {
    function_oid: i64,
    mode: Option<String>,
    name: Option<String>,
    data_type: String,
}

#[derive(Clone, sqlx::FromRow)]
struct PgTriggerRow {
    schema: String,
    table_name: String,
    name: String,
    function_name: String,
    function_schema: String,
    trigger_type: i32,
}

#[derive(Clone, sqlx::FromRow)]
struct PgRowLevelSecurityRow {
    schema: String,
    table_name: String,
    forced: bool,
}

#[derive(Clone, sqlx::FromRow)]
struct PgRowLevelSecurityPolicyRow {
    schema: String,
    table_name: String,
    name: String,
    permissive: bool,
    roles: Vec<String>,
    command: String,
    using_expression: Option<String>,
    check_expression: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct PgPartitionAttachmentRow {
    parent_schema: String,
    parent_table: String,
    child_schema: String,
    child_table: String,
    bound: String,
}

#[derive(Clone, sqlx::FromRow)]
struct PgDefaultPrivilegeRow {
    schema: String,
    owner_role: String,
    object_type: String,
    grantee: String,
    privilege_type: String,
    grantable: bool,
}

#[derive(Clone, sqlx::FromRow)]
struct PgObjectPrivilegeRow {
    schema: String,
    object_type: String,
    object_name: String,
    grantee: String,
    privilege_type: String,
    grantable: bool,
}

#[derive(Clone, sqlx::FromRow)]
struct PgColumnPrivilegeRow {
    schema: String,
    table_name: String,
    column_name: String,
    grantee: String,
    privilege_type: String,
    grantable: bool,
}

#[derive(Clone, sqlx::FromRow)]
struct PgRevokedDefaultPrivilegeRow {
    schema: String,
    owner_role: String,
    object_type: String,
    grantee: String,
    privilege_type: String,
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
            formatted_type,
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
            // `udt_name` omits type modifiers. PostgreSQL extensions such as
            // pgvector use them for dimensions (`halfvec(384)`), so retain the
            // catalog-rendered form when it is available.
            "USER-DEFINED" => formatted_type.unwrap_or(udt_name).to_string(),
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
        let schemas: Vec<String> = sqlx::query_scalar(pg_snapshot_queries::SCHEMAS)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;
        Ok(schemas)
    }
    async fn get_extensions(&self, _schema: &Option<String>) -> Result<Vec<String>> {
        let extensions: Vec<String> = sqlx::query_scalar(pg_snapshot_queries::EXTENSIONS)
            .fetch_all(&self.pool)
            .await?;
        Ok(extensions)
    }

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>> {
        let enum_rows = sqlx::query_as::<_, PgEnumRow>(pg_snapshot_queries::ENUMS)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;

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

    async fn get_composite_types(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, CompositeType>> {
        let type_rows =
            sqlx::query_as::<_, PgCompositeTypeRow>(pg_snapshot_queries::COMPOSITE_TYPES)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;
        let column_rows = sqlx::query_as::<_, PgCompositeTypeColumnRow>(
            pg_snapshot_queries::COMPOSITE_TYPE_COLUMNS,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;
        let columns = composite_type_columns_from_rows(column_rows);

        let mut composite_types = IndexMap::new();
        for row in type_rows {
            let id = Iden::new(row.name.clone(), Some(row.schema.clone()));
            composite_types.insert(
                id.clone(),
                CompositeType {
                    name: row.name,
                    schema: Some(row.schema),
                    columns: columns.get(&id).cloned().unwrap_or_default(),
                    description: row.description,
                },
            );
        }

        Ok(composite_types)
    }

    async fn get_domains(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Domain>> {
        let domain_rows = sqlx::query_as::<_, PgDomainRow>(pg_snapshot_queries::DOMAINS)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;
        let constraint_rows =
            sqlx::query_as::<_, PgDomainConstraintRow>(pg_snapshot_queries::DOMAIN_CONSTRAINTS)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;
        let constraints = domain_constraints_from_rows(constraint_rows);

        let mut domains = IndexMap::new();
        for row in domain_rows {
            let id = Iden::new(row.name.clone(), Some(row.schema.clone()));
            domains.insert(
                id.clone(),
                Domain {
                    name: row.name,
                    schema: Some(row.schema),
                    base_type: DataType::parse(row.base_type, &SqlDialect::Postgres),
                    not_null: row.not_null,
                    default: row.default,
                    constraints: constraints.get(&id).cloned().unwrap_or_default(),
                    description: row.description,
                },
            );
        }

        Ok(domains)
    }

    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>> {
        let rows = sqlx::query_as::<_, PgSequenceRow>(pg_snapshot_queries::SEQUENCES)
            .bind(schema.clone())
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
        let table_rows = sqlx::query_as::<_, PgTableRow>(pg_snapshot_queries::TABLES)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await
            .map_err(ShkiError::database)?;

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
        let rows = sqlx::query_as::<_, PgViewRow>(pg_snapshot_queries::VIEWS)
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
                view.columns.push(crate::schema::ViewColumn::parse(
                    name,
                    data_type,
                    &SqlDialect::Postgres,
                ));
            }
        }

        Ok(views)
    }

    async fn get_functions(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Function>> {
        let function_rows = sqlx::query_as::<_, PgFunctionRow>(pg_snapshot_queries::FUNCTIONS)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;
        let parameter_rows =
            sqlx::query_as::<_, PgFunctionParameterRow>(pg_snapshot_queries::FUNCTION_PARAMETERS)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;
        let parameters = function_parameters_from_rows(parameter_rows);

        let mut functions = IndexMap::new();
        for row in function_rows {
            let signature = format!("{}({})", row.name, row.identity_arguments);
            functions.insert(
                Iden::new(signature.clone(), Some(row.schema.clone())),
                Function {
                    name: row.name,
                    schema: Some(row.schema),
                    signature,
                    parameters: parameters.get(&row.oid).cloned().unwrap_or_default(),
                    return_type: row
                        .return_type
                        .map(|return_type| DataType::parse(return_type, &SqlDialect::Postgres)),
                    language: Some(row.language),
                    body: row.body,
                },
            );
        }

        Ok(functions)
    }

    async fn get_procedures(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Procedure>> {
        let procedure_rows = sqlx::query_as::<_, PgProcedureRow>(pg_snapshot_queries::PROCEDURES)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;
        let parameter_rows =
            sqlx::query_as::<_, PgFunctionParameterRow>(pg_snapshot_queries::PROCEDURE_PARAMETERS)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;
        let parameters = function_parameters_from_rows(parameter_rows);

        let mut procedures = IndexMap::new();
        for row in procedure_rows {
            let signature = format!("{}({})", row.name, row.identity_arguments);
            procedures.insert(
                Iden::new(signature.clone(), Some(row.schema.clone())),
                Procedure {
                    name: row.name,
                    schema: Some(row.schema),
                    signature,
                    parameters: parameters.get(&row.oid).cloned().unwrap_or_default(),
                    language: Some(row.language),
                    body: row.body,
                },
            );
        }

        Ok(procedures)
    }

    async fn get_aggregates(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Aggregate>> {
        let aggregate_rows = sqlx::query_as::<_, PgAggregateRow>(pg_snapshot_queries::AGGREGATES)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;
        let parameter_rows =
            sqlx::query_as::<_, PgFunctionParameterRow>(pg_snapshot_queries::AGGREGATE_PARAMETERS)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;
        let parameters = function_parameters_from_rows(parameter_rows);

        let mut aggregates = IndexMap::new();
        for row in aggregate_rows {
            let signature = format!("{}({})", row.name, row.identity_arguments);
            aggregates.insert(
                Iden::new(signature.clone(), Some(row.schema.clone())),
                Aggregate {
                    name: row.name,
                    schema: Some(row.schema),
                    signature,
                    parameters: parameters.get(&row.oid).cloned().unwrap_or_default(),
                    return_type: DataType::parse(row.return_type, &SqlDialect::Postgres),
                    state_type: DataType::parse(row.state_type, &SqlDialect::Postgres),
                    transition_function: optional_iden(
                        row.transition_function_name,
                        row.transition_function_schema,
                    ),
                    final_function: optional_iden(
                        row.final_function_name,
                        row.final_function_schema,
                    ),
                    initial_condition: row.initial_condition,
                },
            );
        }

        Ok(aggregates)
    }

    async fn get_triggers(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Trigger>> {
        let rows = sqlx::query_as::<_, PgTriggerRow>(pg_snapshot_queries::TRIGGERS)
            .bind(schema.clone())
            .fetch_all(&self.pool)
            .await?;

        Ok(triggers_from_rows(rows))
    }

    async fn get_row_level_security(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, RowLevelSecurity>> {
        let rows =
            sqlx::query_as::<_, PgRowLevelSecurityRow>(pg_snapshot_queries::ROW_LEVEL_SECURITY)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let table = Iden::new(row.table_name.clone(), Some(row.schema.clone()));
                (
                    table.clone(),
                    RowLevelSecurity {
                        table,
                        forced: row.forced,
                    },
                )
            })
            .collect())
    }

    async fn get_row_level_security_policies(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, RowLevelSecurityPolicy>> {
        let rows = sqlx::query_as::<_, PgRowLevelSecurityPolicyRow>(
            pg_snapshot_queries::ROW_LEVEL_SECURITY_POLICIES,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id = Iden::new(
                    format!("{}.{}", row.table_name, row.name),
                    Some(row.schema.clone()),
                );
                (
                    id,
                    RowLevelSecurityPolicy {
                        name: row.name,
                        table: Iden::new(row.table_name, Some(row.schema)),
                        permissive: row.permissive,
                        roles: row.roles,
                        command: row.command,
                        using_expression: row.using_expression,
                        check_expression: row.check_expression,
                    },
                )
            })
            .collect())
    }

    async fn get_partition_attachments(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, PartitionAttachment>> {
        let rows = sqlx::query_as::<_, PgPartitionAttachmentRow>(
            pg_snapshot_queries::PARTITION_ATTACHMENTS,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id = Iden::new(
                    format!("{}.{}", row.parent_table, row.child_table),
                    Some(row.parent_schema.clone()),
                );
                (
                    id,
                    PartitionAttachment {
                        parent: Iden::new(row.parent_table, Some(row.parent_schema)),
                        child: Iden::new(row.child_table, Some(row.child_schema)),
                        bound: row.bound,
                    },
                )
            })
            .collect())
    }

    async fn get_default_privileges(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<DefaultPrivilege>>> {
        let rows =
            sqlx::query_as::<_, PgDefaultPrivilegeRow>(pg_snapshot_queries::DEFAULT_PRIVILEGES)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;

        Ok(default_privileges_from_rows(rows))
    }

    async fn get_object_privileges(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<ObjectPrivilege>>> {
        let rows =
            sqlx::query_as::<_, PgObjectPrivilegeRow>(pg_snapshot_queries::OBJECT_PRIVILEGES)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;

        Ok(object_privileges_from_rows(rows))
    }

    async fn get_column_privileges(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<ColumnPrivilege>>> {
        let rows =
            sqlx::query_as::<_, PgColumnPrivilegeRow>(pg_snapshot_queries::COLUMN_PRIVILEGES)
                .bind(schema.clone())
                .fetch_all(&self.pool)
                .await?;

        Ok(column_privileges_from_rows(rows))
    }

    async fn get_revoked_default_privileges(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<RevokedDefaultPrivilege>>> {
        let rows = sqlx::query_as::<_, PgRevokedDefaultPrivilegeRow>(
            pg_snapshot_queries::REVOKED_DEFAULT_PRIVILEGES,
        )
        .bind(schema.clone())
        .fetch_all(&self.pool)
        .await?;

        Ok(revoked_default_privileges_from_rows(rows))
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Column>>> {
        let rows = sqlx::query_as::<_, PgInfoSchemaColumnRow>(pg_snapshot_queries::COLUMNS)
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
        let rows = sqlx::query_as::<_, PgConstraintRow>(pg_snapshot_queries::CONSTRAINTS)
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
        let rows = sqlx::query_as::<_, PgIndexRow>(pg_snapshot_queries::INDEXES)
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

        let method = parse_index_method(&row.index_method);
        let index = index_map
            .entry(row.index_name.clone())
            .or_insert_with(|| Index {
                name: row.index_name.clone(),
                columns: Vec::new(),
                unique: row.is_unique,
                method,
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
                order: parse_index_sort_order(index.method, row.sort_order.as_deref()),
                nulls: parse_index_nulls_order(index.method, row.nulls_order.as_deref()),
            }
        } else if let Some(column_name) = row.column_name.as_ref() {
            IndexColumn::Column {
                name: column_name.clone(),
                order: parse_index_sort_order(index.method, row.sort_order.as_deref()),
                nulls: parse_index_nulls_order(index.method, row.nulls_order.as_deref()),
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

fn composite_type_columns_from_rows(
    rows: Vec<PgCompositeTypeColumnRow>,
) -> IndexMap<Iden, Vec<CompositeTypeColumn>> {
    let mut columns = IndexMap::new();

    for row in rows {
        columns
            .entry(Iden::new(row.type_name, Some(row.schema)))
            .or_insert_with(Vec::new)
            .push(CompositeTypeColumn {
                name: row.column_name,
                data_type: DataType::parse(row.data_type, &SqlDialect::Postgres),
            });
    }

    columns
}

fn domain_constraints_from_rows(
    rows: Vec<PgDomainConstraintRow>,
) -> IndexMap<Iden, Vec<DomainConstraint>> {
    let mut constraints = IndexMap::new();

    for row in rows {
        constraints
            .entry(Iden::new(row.domain_name, Some(row.schema)))
            .or_insert_with(Vec::new)
            .push(DomainConstraint {
                name: row.constraint_name,
                definition: row.constraint_definition,
            });
    }

    constraints
}

fn optional_iden(name: Option<String>, schema: Option<String>) -> Option<Iden> {
    name.filter(|name| !name.is_empty())
        .map(|name| Iden::new(name, schema))
}

fn function_parameters_from_rows(
    rows: Vec<PgFunctionParameterRow>,
) -> IndexMap<i64, Vec<FunctionParameter>> {
    let mut parameters = IndexMap::new();

    for row in rows {
        parameters
            .entry(row.function_oid)
            .or_insert_with(Vec::new)
            .push(FunctionParameter {
                name: row.name.filter(|name| !name.is_empty()),
                data_type: DataType::parse(row.data_type, &SqlDialect::Postgres),
                mode: row.mode.as_deref().and_then(parse_function_parameter_mode),
            });
    }

    parameters
}

fn default_privileges_from_rows(
    rows: Vec<PgDefaultPrivilegeRow>,
) -> IndexMap<String, Vec<DefaultPrivilege>> {
    let mut privileges = IndexMap::new();
    for row in rows {
        privileges
            .entry(row.schema)
            .or_insert_with(Vec::new)
            .push(DefaultPrivilege {
                owner_role: row.owner_role,
                object_type: row.object_type,
                grantee: row.grantee,
                privilege_type: row.privilege_type,
                grantable: row.grantable,
            });
    }
    privileges
}

fn object_privileges_from_rows(
    rows: Vec<PgObjectPrivilegeRow>,
) -> IndexMap<String, Vec<ObjectPrivilege>> {
    let mut privileges = IndexMap::new();
    for row in rows {
        let schema = row.schema;
        privileges
            .entry(schema.clone())
            .or_insert_with(Vec::new)
            .push(ObjectPrivilege {
                object_type: row.object_type,
                object: Iden::new(row.object_name, Some(schema)),
                grantee: row.grantee,
                privilege_type: row.privilege_type,
                grantable: row.grantable,
            });
    }
    privileges
}

fn column_privileges_from_rows(
    rows: Vec<PgColumnPrivilegeRow>,
) -> IndexMap<String, Vec<ColumnPrivilege>> {
    let mut privileges = IndexMap::new();
    for row in rows {
        let schema = row.schema;
        privileges
            .entry(schema.clone())
            .or_insert_with(Vec::new)
            .push(ColumnPrivilege {
                table: Iden::new(row.table_name, Some(schema)),
                column: row.column_name,
                grantee: row.grantee,
                privilege_type: row.privilege_type,
                grantable: row.grantable,
            });
    }
    privileges
}

fn revoked_default_privileges_from_rows(
    rows: Vec<PgRevokedDefaultPrivilegeRow>,
) -> IndexMap<String, Vec<RevokedDefaultPrivilege>> {
    let mut privileges = IndexMap::new();
    for row in rows {
        privileges
            .entry(row.schema)
            .or_insert_with(Vec::new)
            .push(RevokedDefaultPrivilege {
                owner_role: row.owner_role,
                object_type: row.object_type,
                grantee: row.grantee,
                privilege_type: row.privilege_type,
            });
    }
    privileges
}

fn parse_function_parameter_mode(mode: &str) -> Option<FunctionParameterMode> {
    match mode {
        "IN" => Some(FunctionParameterMode::In),
        "OUT" => Some(FunctionParameterMode::Out),
        "INOUT" => Some(FunctionParameterMode::InOut),
        "VARIADIC" => Some(FunctionParameterMode::Variadic),
        _ => None,
    }
}

fn triggers_from_rows(rows: Vec<PgTriggerRow>) -> IndexMap<Iden, Trigger> {
    rows.into_iter()
        .map(|row| {
            let id = Iden::new(row.name.clone(), Some(row.schema.clone()));
            (
                id,
                Trigger {
                    name: row.name,
                    table: Iden::new(row.table_name, Some(row.schema)),
                    function: Iden::new(row.function_name, Some(row.function_schema)),
                    events: trigger_events(row.trigger_type),
                    timing: trigger_timing(row.trigger_type),
                    orientation: trigger_orientation(row.trigger_type),
                },
            )
        })
        .collect()
}

fn trigger_events(trigger_type: i32) -> Vec<TriggerEvent> {
    let mut events = Vec::new();
    if trigger_type & 4 != 0 {
        events.push(TriggerEvent::Insert);
    }
    if trigger_type & 8 != 0 {
        events.push(TriggerEvent::Delete);
    }
    if trigger_type & 16 != 0 {
        events.push(TriggerEvent::Update);
    }
    if trigger_type & 32 != 0 {
        events.push(TriggerEvent::Truncate);
    }
    events
}

fn trigger_timing(trigger_type: i32) -> Option<TriggerTiming> {
    if trigger_type & 2 != 0 {
        Some(TriggerTiming::Before)
    } else if trigger_type & 64 != 0 {
        Some(TriggerTiming::InsteadOf)
    } else {
        Some(TriggerTiming::After)
    }
}

fn trigger_orientation(trigger_type: i32) -> Option<TriggerOrientation> {
    if trigger_type & 1 != 0 {
        Some(TriggerOrientation::Row)
    } else {
        Some(TriggerOrientation::Statement)
    }
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

fn parse_index_sort_order(method: IndexMethod, order: Option<&str>) -> Option<SortOrder> {
    index_method_supports_ordering(method).then(|| parse_sort_order(order))?
}

fn parse_index_nulls_order(method: IndexMethod, order: Option<&str>) -> Option<NullsOrder> {
    index_method_supports_ordering(method).then(|| parse_nulls_order(order))?
}

fn index_method_supports_ordering(method: IndexMethod) -> bool {
    matches!(method, IndexMethod::BTree)
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
    use crate::sql::statements::create_table;

    fn base_column_row(name: &str, data_type: &str) -> PgInfoSchemaColumnRow {
        PgInfoSchemaColumnRow {
            table_schema: "public".to_string(),
            table_name: "users".to_string(),
            column_name: name.to_string(),
            data_type: data_type.to_string(),
            udt_name: data_type.to_string(),
            formatted_type: None,
            is_nullable: "YES".to_string(),
            column_default: None,
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
            owned_sequence_schema: None,
            owned_sequence_name: None,
            owned_sequence_increment: None,
            owned_sequence_min_value: None,
            owned_sequence_max_value: None,
            owned_sequence_start: None,
            owned_sequence_cache: None,
            owned_sequence_cycle: None,
        }
    }

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
    fn object_privileges_query_omits_owner_privileges() {
        assert!(
            pg_snapshot_queries::OBJECT_PRIVILEGES
                .contains("tp.grantee <> pg_get_userbyid(c.relowner)")
        );
    }

    #[test]
    fn column_privileges_query_omits_owner_privileges() {
        assert!(
            pg_snapshot_queries::COLUMN_PRIVILEGES
                .contains("cp.grantee <> pg_get_userbyid(c.relowner)")
        );
    }

    #[test]
    fn renders_text_default_named_default_as_string_literal() {
        let row = PgInfoSchemaColumnRow {
            column_default: Some("'default'::text".to_string()),
            is_nullable: "NO".to_string(),
            ..base_column_row("indexing", "text")
        };
        let mut table = Table::in_schema("item", "public");
        table.column(Column::from(row));

        let sql = create_table(&SqlDialect::Postgres, &table).to_string(None);

        assert!(sql.contains("\"indexing\" TEXT NOT NULL DEFAULT 'default'"));
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
            formatted_type: None,
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
            formatted_type: None,
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
            formatted_type: None,
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
            formatted_type: None,
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
    fn non_btree_indexes_drop_sort_and_nulls_metadata_from_rows() {
        let indexes = indexes_from_rows(vec![PgIndexRow {
            table_schema: "public".to_string(),
            table_name: "events".to_string(),
            index_name: "events_payload_idx".to_string(),
            index_method: "gin".to_string(),
            is_unique: false,
            is_constraint: false,
            where_clause: None,
            tablespace: None,
            reloptions: Vec::new(),
            is_include_column: false,
            column_name: Some("payload".to_string()),
            expression: None,
            opclass: Some("jsonb_path_ops".to_string()),
            sort_order: Some("ASC".to_string()),
            nulls_order: Some("LAST".to_string()),
        }])
        .expect("index rows should aggregate successfully");

        let index = indexes
            .get(&Iden::new("events", Some("public".to_string())))
            .and_then(|table_indexes| table_indexes.get("events_payload_idx"))
            .expect("gin index should be present");

        assert_eq!(index.method, IndexMethod::Gin);
        assert!(matches!(
            index.columns.as_slice(),
            [IndexColumn::Column {
                name,
                order: None,
                nulls: None,
                opclass: Some(opclass),
            }] if name == "payload" && opclass == "jsonb_path_ops"
        ));
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

    #[test]
    fn postgres_column_rows_introspect_column_metadata() {
        let mut identity_row = base_column_row("id", "integer");
        identity_row.is_nullable = "NO".to_string();
        identity_row.is_identity = "YES".to_string();
        identity_row.identity_generation = Some("ALWAYS".to_string());
        identity_row.identity_start = Some("10".to_string());
        identity_row.identity_increment = Some("5".to_string());

        let column = Column::from(identity_row);
        assert_eq!(column.data_type, DataType::Integer);
        assert!(!column.nullable);
        assert!(matches!(
            column.identity,
            Some(IdentitySpec {
                always: true,
                sequence_options: Some(SequenceOptions {
                    start: Some(10),
                    increment: Some(5),
                    ..
                })
            })
        ));

        let mut generated_row = base_column_row("name_lower", "text");
        generated_row.is_generated = "ALWAYS".to_string();
        generated_row.generation_expression = Some("lower(name)".to_string());

        let generated = Column::from(generated_row);
        assert!(matches!(
            generated.generated,
            Some(GeneratedColumn { stored: true, ref expression }) if expression == "lower(name)"
        ));
    }

    #[test]
    fn postgres_constraint_rows_aggregate_table_constraints() {
        let constraints = {
            let rows = vec![
                PgConstraintRow {
                    table_schema: "public".to_string(),
                    table_name: "posts".to_string(),
                    constraint_name: "posts_pkey".to_string(),
                    constraint_type: "PRIMARY KEY".to_string(),
                    column_name: Some("id".to_string()),
                    foreign_table_schema: None,
                    foreign_table_name: None,
                    foreign_column_name: None,
                    update_action: None,
                    delete_action: None,
                    deferrable: false,
                    initially_deferred: false,
                    constraint_expression: None,
                },
                PgConstraintRow {
                    table_schema: "public".to_string(),
                    table_name: "posts".to_string(),
                    constraint_name: "posts_tenant_slug_key".to_string(),
                    constraint_type: "UNIQUE".to_string(),
                    column_name: Some("tenant_id".to_string()),
                    foreign_table_schema: None,
                    foreign_table_name: None,
                    foreign_column_name: None,
                    update_action: None,
                    delete_action: None,
                    deferrable: false,
                    initially_deferred: false,
                    constraint_expression: None,
                },
                PgConstraintRow {
                    table_schema: "public".to_string(),
                    table_name: "posts".to_string(),
                    constraint_name: "posts_tenant_slug_key".to_string(),
                    constraint_type: "UNIQUE".to_string(),
                    column_name: Some("slug".to_string()),
                    foreign_table_schema: None,
                    foreign_table_name: None,
                    foreign_column_name: None,
                    update_action: None,
                    delete_action: None,
                    deferrable: false,
                    initially_deferred: false,
                    constraint_expression: None,
                },
                PgConstraintRow {
                    table_schema: "public".to_string(),
                    table_name: "posts".to_string(),
                    constraint_name: "posts_user_id_fkey".to_string(),
                    constraint_type: "FOREIGN KEY".to_string(),
                    column_name: Some("user_id".to_string()),
                    foreign_table_schema: Some("public".to_string()),
                    foreign_table_name: Some("users".to_string()),
                    foreign_column_name: Some("id".to_string()),
                    update_action: Some("r".to_string()),
                    delete_action: Some("c".to_string()),
                    deferrable: true,
                    initially_deferred: true,
                    constraint_expression: None,
                },
                PgConstraintRow {
                    table_schema: "public".to_string(),
                    table_name: "posts".to_string(),
                    constraint_name: "posts_title_check".to_string(),
                    constraint_type: "CHECK".to_string(),
                    column_name: None,
                    foreign_table_schema: None,
                    foreign_table_name: None,
                    foreign_column_name: None,
                    update_action: None,
                    delete_action: None,
                    deferrable: false,
                    initially_deferred: false,
                    constraint_expression: Some("CHECK (length(title) > 0)".to_string()),
                },
            ];

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
                            references: Iden::new(
                                row.foreign_table_name.clone().unwrap_or_default(),
                                row.foreign_table_schema.clone(),
                            ),
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
                        if let Some(column_name) = row.column_name.as_ref() {
                            constraint.columns.push(column_name.clone());
                        }
                    }
                    Constraint::Unique(constraint) => {
                        if let Some(column_name) = row.column_name.as_ref() {
                            constraint.columns.push(column_name.clone());
                        }
                    }
                    Constraint::ForeignKey(constraint) => {
                        if let Some(column_name) = row.column_name.as_ref() {
                            constraint.columns.push(column_name.clone());
                        }
                        if let Some(foreign_column_name) = row.foreign_column_name.as_ref() {
                            constraint
                                .references_columns
                                .push(foreign_column_name.clone());
                        }
                    }
                    Constraint::Check(_) | Constraint::Exclusion(_) => {}
                }
            }
            constraints_by_table
                .into_iter()
                .map(|(table_id, constraints)| (table_id, constraints.into_values().collect()))
                .collect::<IndexMap<_, Vec<_>>>()
        };

        let table_constraints = constraints
            .get(&Iden::new("posts", Some("public".to_string())))
            .expect("posts constraints should exist");

        assert!(table_constraints.iter().any(
            |constraint| matches!(constraint, Constraint::PrimaryKey(pk) if pk.columns == vec!["id"])
        ));
        assert!(table_constraints.iter().any(
            |constraint| matches!(constraint, Constraint::Unique(unique) if unique.columns == vec!["tenant_id", "slug"])
        ));
        assert!(table_constraints.iter().any(|constraint| {
            matches!(
                constraint,
                Constraint::ForeignKey(fk)
                    if fk.references == Iden::new("users", Some("public".to_string()))
                        && fk.columns == vec!["user_id"]
                        && fk.references_columns == vec!["id"]
                        && fk.on_delete == crate::schema::ReferenceAction::Cascade
                        && fk.on_update == crate::schema::ReferenceAction::Restrict
                        && fk.deferrable
                        && fk.initially_deferred
            )
        }));
        assert!(table_constraints.iter().any(
            |constraint| matches!(constraint, Constraint::Check(check) if check.expression == "length(title) > 0")
        ));
    }
}
