use crate::diff::diff_snapshots;
use crate::engines::Engine;
use crate::models::iden::Iden;
use crate::schema::{
    Aggregate, CatalogSchema, ColumnPrivilege, CompositeType, DefaultPrivilege, Domain, Function,
    FunctionParameterMode, ObjectPrivilege, PartitionAttachment, Procedure,
    RevokedDefaultPrivilege, RowLevelSecurity, RowLevelSecurityPolicy, SqlDialect, Table,
    TriggerEvent, TriggerOrientation, TriggerTiming,
};
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::render::SqlRenderer;
use crate::sql::statements::{
    create_enum, create_extension, create_index, create_sequence, create_table, create_view,
    qualified_name, quote_identifier,
};
use crate::utils::resolve_path;
use crate::{Config, Result, ShkiError};

use owo_colors::OwoColorize;
use serde::Serialize;
use std::path::{Path, PathBuf};

use super::display::preview::{PreviewFile, render_preview};

#[derive(Debug, clap::ValueEnum, Default, Clone, Serialize)]
#[value(rename_all = "lowercase")]
pub enum SchemaExportFormat {
    Json,
    #[default]
    Sql,
}

pub async fn cmd_dump(
    config: &Config,
    format: &SchemaExportFormat,
    output: Option<&std::path::Path>,
    dirs: bool,
    force: bool,
    schema: &Option<String>,
) -> Result<()> {
    export_live_schema(config, format, output, dirs, force, schema, "Dump").await
}

pub async fn export_live_schema(
    config: &Config,
    format: &SchemaExportFormat,
    output: Option<&std::path::Path>,
    dirs: bool,
    force: bool,
    schema: &Option<String>,
    workflow_name: &str,
) -> Result<()> {
    config.display_sanitized_db_url();

    println!(
        "{}",
        format!("{workflow_name}ing database shape...\n").cyan()
    );

    let engine = Engine::from_config(config).await?;
    let snapshot = engine.introspect(config, schema).await?;

    if dirs {
        if !matches!(format, SchemaExportFormat::Sql) {
            return Err(ShkiError::config(
                "Directory Schema output requires --format sql",
            ));
        }

        if let Some(output) = output {
            let output = resolve_path(Some(config.root.clone()), output);
            write_directory_schema(&snapshot, &output, force)?;
            println!("{} {}", "Schema written to:".green(), output.display());
        } else {
            println!("{}", render_directory_schema_preview(config, &snapshot)?);
        }
        return Ok(());
    }

    let content = render_snapshot(&snapshot, format)?;

    match output {
        Some(path) => {
            let resolved_path = resolve_path(Some(config.root.clone()), path);
            std::fs::write(&resolved_path, &content)?;
            println!(
                "{} {}",
                "Schema written to:".green(),
                resolved_path.display()
            );
        }
        None => {
            println!("{}", content);
        }
    }

    Ok(())
}

pub fn write_directory_schema(snapshot: &Snapshot, output: &Path, force: bool) -> Result<()> {
    if output.exists() && !output.is_dir() {
        return Err(ShkiError::config(format!(
            "Directory Schema output must be a directory: {}",
            output.display()
        )));
    }

    let files = directory_schema_files(snapshot)?;
    let collisions: Vec<PathBuf> = files
        .iter()
        .map(|file| output.join(&file.path))
        .filter(|path| path.exists())
        .collect();

    if !force && !collisions.is_empty() {
        return Err(ShkiError::config(format!(
            "Directory Schema output would overwrite existing file: {}",
            collisions[0].display()
        )));
    }

    for file in files {
        let path = output.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, file.content)?;
    }

    Ok(())
}

pub fn render_directory_schema_preview(config: &Config, snapshot: &Snapshot) -> Result<String> {
    let files: Vec<PreviewFile> = directory_schema_files(snapshot)?
        .into_iter()
        .map(|f| PreviewFile {
            path: f.path.to_string_lossy().to_string(),
            content: f.content,
        })
        .collect();
    let output = render_preview(&files, "sql", config.no_color());
    Ok(output)
}

#[derive(Debug)]
struct DirectorySchemaFile {
    path: PathBuf,
    content: String,
}

fn directory_schema_files(snapshot: &Snapshot) -> Result<Vec<DirectorySchemaFile>> {
    let mut files = Vec::new();
    let mut includes = Vec::new();

    for extension in snapshot.catalog.extensions.values() {
        let path = PathBuf::from("extensions")
            .join(format!("{}.sql", sanitize_file_name(&extension.name)));
        files.push(DirectorySchemaFile {
            path: path.clone(),
            content: create_extension(&snapshot.dialect, &extension.name).to_string(),
        });
        includes.push(path);
    }

    for schema in snapshot.catalog.schemas.values() {
        render_schema_directory(snapshot, schema, &mut files, &mut includes)?;
    }

    let main = includes
        .iter()
        .map(|path| format!("\\i {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    files.insert(
        0,
        DirectorySchemaFile {
            path: PathBuf::from("main.sql"),
            content: if main.is_empty() {
                main
            } else {
                format!("{}\n", main)
            },
        },
    );

    Ok(files)
}

fn render_schema_directory(
    snapshot: &Snapshot,
    schema: &CatalogSchema,
    files: &mut Vec<DirectorySchemaFile>,
    includes: &mut Vec<PathBuf>,
) -> Result<()> {
    let schema_root = PathBuf::from(&schema.name);

    if !matches!(schema.name.as_str(), "public" | "main") {
        push_file(
            files,
            includes,
            schema_root.join("schema.sql"),
            format!(
                "CREATE SCHEMA {};",
                quote_identifier(&snapshot.dialect, &schema.name)
            ),
        );
    }

    for db_enum in schema.enums.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("types")
                .join(format!("{}.sql", sanitize_file_name(&db_enum.name))),
            create_enum(
                &snapshot.dialect,
                &db_enum.name,
                &db_enum.schema,
                &db_enum.values,
                &db_enum.description,
            )
            .to_string(None),
        );
    }

    for composite_type in schema.composite_types.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("types")
                .join(format!("{}.sql", sanitize_file_name(&composite_type.name))),
            render_composite_type(&snapshot.dialect, composite_type)?,
        );
    }

    for domain in schema.domains.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("types")
                .join(format!("{}.sql", sanitize_file_name(&domain.name))),
            render_domain(&snapshot.dialect, domain)?,
        );
    }

    for sequence in schema.sequences.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("sequences")
                .join(format!("{}.sql", sanitize_file_name(&sequence.name))),
            create_sequence(&snapshot.dialect, sequence).to_string(),
        );
    }

    for table in schema.tables.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("tables")
                .join(format!("{}.sql", sanitize_file_name(&table.name))),
            render_table_file(&snapshot.dialect, schema, table)?,
        );
    }

    if !schema.default_privileges.is_empty() {
        push_file(
            files,
            includes,
            schema_root.join("privileges").join("default.sql"),
            render_default_privileges(&snapshot.dialect, &schema.name, &schema.default_privileges),
        );
    }

    if !schema.revoked_default_privileges.is_empty() {
        push_file(
            files,
            includes,
            schema_root.join("privileges").join("revoked_default.sql"),
            render_revoked_default_privileges(
                &snapshot.dialect,
                &schema.name,
                &schema.revoked_default_privileges,
            ),
        );
    }

    for view in schema.views.values() {
        let dir = if view.materialized {
            "materialized_views"
        } else {
            "views"
        };
        push_file(
            files,
            includes,
            schema_root
                .join(dir)
                .join(format!("{}.sql", sanitize_file_name(&view.name))),
            create_view(&snapshot.dialect, view, false).to_string(),
        );
    }

    for function in schema.functions.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("functions")
                .join(format!("{}.sql", sanitize_file_name(&function.signature))),
            render_function(&snapshot.dialect, function)?,
        );
    }

    for procedure in schema.procedures.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("procedures")
                .join(format!("{}.sql", sanitize_file_name(&procedure.signature))),
            render_procedure(&snapshot.dialect, procedure)?,
        );
    }

    for aggregate in schema.aggregates.values() {
        push_file(
            files,
            includes,
            schema_root
                .join("aggregates")
                .join(format!("{}.sql", sanitize_file_name(&aggregate.signature))),
            render_aggregate(&snapshot.dialect, aggregate)?,
        );
    }

    for trigger in schema.triggers.values().filter(|trigger| {
        !schema
            .tables
            .values()
            .any(|table| iden_matches_table(&trigger.table, table))
    }) {
        push_file(
            files,
            includes,
            schema_root
                .join("triggers")
                .join(format!("{}.sql", sanitize_file_name(&trigger.name))),
            render_trigger(&snapshot.dialect, trigger)?,
        );
    }

    Ok(())
}

fn render_table_file(
    dialect: &SqlDialect,
    schema: &CatalogSchema,
    table: &Table,
) -> Result<String> {
    let triggers = schema
        .triggers
        .values()
        .filter(|trigger| iden_matches_table(&trigger.table, table));
    let row_level_security = schema
        .row_level_security
        .values()
        .find(|rls| iden_matches_table(&rls.table, table));
    let policies = schema
        .row_level_security_policies
        .values()
        .filter(|policy| iden_matches_table(&policy.table, table));
    let partition_attachments = schema
        .partition_attachments
        .values()
        .filter(|attachment| iden_matches_table(&attachment.child, table));
    let object_privileges = schema
        .object_privileges
        .iter()
        .filter(|privilege| iden_matches_table(&privilege.object, table));
    let column_privileges = schema
        .column_privileges
        .iter()
        .filter(|privilege| iden_matches_table(&privilege.table, table));

    let mut statements = vec![create_table(dialect, table).to_string(None)];

    for index in table.indexes.values() {
        statements.push(create_index(dialect, &table.name, &table.schema, index).to_string());
    }

    for trigger in triggers {
        statements.push(render_trigger(dialect, trigger)?);
    }

    if let Some(row_level_security) = row_level_security {
        statements.extend(render_row_level_security(dialect, row_level_security));
    }

    for policy in policies {
        statements.push(render_row_level_security_policy(dialect, policy));
    }

    for attachment in partition_attachments {
        statements.push(render_partition_attachment(dialect, attachment));
    }

    for privilege in object_privileges {
        statements.push(render_object_privilege(dialect, privilege));
    }

    for privilege in column_privileges {
        statements.push(render_column_privilege(dialect, privilege));
    }

    Ok(statements.join("\n\n"))
}

fn render_row_level_security(
    dialect: &SqlDialect,
    row_level_security: &RowLevelSecurity,
) -> Vec<String> {
    let table = qualified_iden(dialect, &row_level_security.table);
    let mut statements = vec![format!("ALTER TABLE {} ENABLE ROW LEVEL SECURITY;", table)];
    if row_level_security.forced {
        statements.push(format!("ALTER TABLE {} FORCE ROW LEVEL SECURITY;", table));
    }
    statements
}

fn render_row_level_security_policy(
    dialect: &SqlDialect,
    policy: &RowLevelSecurityPolicy,
) -> String {
    let mut sql = format!(
        "CREATE POLICY {} ON {}\n    AS {}\n    FOR {}",
        quote_identifier(dialect, &policy.name),
        qualified_iden(dialect, &policy.table),
        if policy.permissive {
            "PERMISSIVE"
        } else {
            "RESTRICTIVE"
        },
        policy.command
    );

    if !policy.roles.is_empty() {
        sql.push_str(&format!(
            "\n    TO {}",
            render_roles(dialect, &policy.roles)
        ));
    }
    if let Some(using_expression) = &policy.using_expression {
        sql.push_str(&format!("\n    USING ({})", using_expression));
    }
    if let Some(check_expression) = &policy.check_expression {
        sql.push_str(&format!("\n    WITH CHECK ({})", check_expression));
    }
    sql.push(';');
    sql
}

fn render_partition_attachment(dialect: &SqlDialect, attachment: &PartitionAttachment) -> String {
    format!(
        "ALTER TABLE {} ATTACH PARTITION {} {};",
        qualified_iden(dialect, &attachment.parent),
        qualified_iden(dialect, &attachment.child),
        attachment.bound
    )
}

fn render_default_privileges(
    dialect: &SqlDialect,
    schema: &str,
    privileges: &[DefaultPrivilege],
) -> String {
    privileges
        .iter()
        .map(|privilege| {
            format!(
                "ALTER DEFAULT PRIVILEGES FOR ROLE {} IN SCHEMA {} GRANT {} ON {} TO {}{};",
                render_role(dialect, &privilege.owner_role),
                quote_identifier(dialect, schema),
                privilege.privilege_type,
                privilege.object_type,
                render_role(dialect, &privilege.grantee),
                if privilege.grantable {
                    " WITH GRANT OPTION"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_object_privilege(dialect: &SqlDialect, privilege: &ObjectPrivilege) -> String {
    format!(
        "GRANT {} ON {} {} TO {}{};",
        privilege.privilege_type,
        grant_object_type(&privilege.object_type),
        qualified_iden(dialect, &privilege.object),
        render_role(dialect, &privilege.grantee),
        if privilege.grantable {
            " WITH GRANT OPTION"
        } else {
            ""
        }
    )
}

fn render_column_privilege(dialect: &SqlDialect, privilege: &ColumnPrivilege) -> String {
    format!(
        "GRANT {} ({}) ON TABLE {} TO {}{};",
        privilege.privilege_type,
        quote_identifier(dialect, &privilege.column),
        qualified_iden(dialect, &privilege.table),
        render_role(dialect, &privilege.grantee),
        if privilege.grantable {
            " WITH GRANT OPTION"
        } else {
            ""
        }
    )
}

fn render_revoked_default_privileges(
    dialect: &SqlDialect,
    schema: &str,
    privileges: &[RevokedDefaultPrivilege],
) -> String {
    privileges
        .iter()
        .map(|privilege| {
            format!(
                "ALTER DEFAULT PRIVILEGES FOR ROLE {} IN SCHEMA {} REVOKE {} ON {} FROM {};",
                render_role(dialect, &privilege.owner_role),
                quote_identifier(dialect, schema),
                privilege.privilege_type,
                privilege.object_type,
                render_role(dialect, &privilege.grantee)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn grant_object_type(object_type: &str) -> &str {
    match object_type {
        "BASE TABLE" | "FOREIGN TABLE" | "LOCAL TEMPORARY" | "VIEW" => "TABLE",
        value => value,
    }
}

fn render_roles(dialect: &SqlDialect, roles: &[String]) -> String {
    roles
        .iter()
        .map(|role| render_role(dialect, role))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_role(dialect: &SqlDialect, role: &str) -> String {
    if role.eq_ignore_ascii_case("public") {
        "PUBLIC".to_string()
    } else {
        quote_identifier(dialect, role)
    }
}

fn iden_matches_table(iden: &Iden, table: &Table) -> bool {
    iden.name == table.name && (iden.schema.is_none() || iden.schema == table.schema)
}

fn push_file(
    files: &mut Vec<DirectorySchemaFile>,
    includes: &mut Vec<PathBuf>,
    path: PathBuf,
    content: String,
) {
    files.push(DirectorySchemaFile {
        path: path.clone(),
        content: ensure_trailing_newline(content),
    });
    includes.push(path);
}

fn render_composite_type(dialect: &SqlDialect, composite_type: &CompositeType) -> Result<String> {
    if composite_type.columns.is_empty() {
        return Err(ShkiError::schema(format!(
            "Cannot render composite type '{}' without columns",
            composite_type.name
        )));
    }

    let columns = composite_type
        .columns
        .iter()
        .map(|column| {
            format!(
                "    {} {}",
                quote_identifier(dialect, &column.name),
                column.data_type.to_string(dialect)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!(
        "{}\nCREATE TYPE {} AS (\n{}\n);",
        advanced_object_notice("composite type"),
        qualified_name(dialect, &composite_type.name, &composite_type.schema),
        columns
    ))
}

fn render_domain(dialect: &SqlDialect, domain: &Domain) -> Result<String> {
    let mut sql = format!(
        "{}\nCREATE DOMAIN {} AS {}",
        advanced_object_notice("domain"),
        qualified_name(dialect, &domain.name, &domain.schema),
        domain.base_type.to_string(dialect)
    );
    if domain.not_null {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = &domain.default {
        sql.push_str(&format!(" DEFAULT {}", default));
    }
    for constraint in &domain.constraints {
        sql.push_str(&format!(
            " CONSTRAINT {} {}",
            quote_identifier(dialect, &constraint.name),
            constraint.definition
        ));
    }
    sql.push(';');
    Ok(sql)
}

fn render_function(dialect: &SqlDialect, function: &Function) -> Result<String> {
    let return_type = function.return_type.as_ref().ok_or_else(|| {
        ShkiError::schema(format!(
            "Cannot render function '{}' without a return type",
            function.signature
        ))
    })?;
    let language = required_routine_language(
        "function",
        &function.signature,
        function.language.as_deref(),
    )?;
    let body = required_routine_body("function", &function.signature, function.body.as_deref())?;
    let dollar_tag = dollar_quote_tag(body);

    Ok(format!(
        "{}\nCREATE FUNCTION {}({})\nRETURNS {}\nLANGUAGE {}\nAS ${}$\n{}\n${}$;",
        advanced_object_notice("function"),
        qualified_name(dialect, &function.name, &function.schema),
        render_parameters(dialect, &function.parameters),
        return_type.to_string(dialect),
        quote_identifier(dialect, language),
        dollar_tag,
        body.trim(),
        dollar_tag
    ))
}

fn render_procedure(dialect: &SqlDialect, procedure: &Procedure) -> Result<String> {
    let language = required_routine_language(
        "procedure",
        &procedure.signature,
        procedure.language.as_deref(),
    )?;
    let body = required_routine_body("procedure", &procedure.signature, procedure.body.as_deref())?;
    let dollar_tag = dollar_quote_tag(body);

    Ok(format!(
        "{}\nCREATE PROCEDURE {}({})\nLANGUAGE {}\nAS ${}$\n{}\n${}$;",
        advanced_object_notice("procedure"),
        qualified_name(dialect, &procedure.name, &procedure.schema),
        render_parameters(dialect, &procedure.parameters),
        quote_identifier(dialect, language),
        dollar_tag,
        body.trim(),
        dollar_tag
    ))
}

fn render_aggregate(dialect: &SqlDialect, aggregate: &Aggregate) -> Result<String> {
    let transition_function = aggregate.transition_function.as_ref().ok_or_else(|| {
        ShkiError::schema(format!(
            "Cannot render aggregate '{}' without a transition function",
            aggregate.signature
        ))
    })?;
    let mut options = vec![
        format!("SFUNC = {}", qualified_iden(dialect, transition_function)),
        format!("STYPE = {}", aggregate.state_type.to_string(dialect)),
    ];
    if let Some(final_function) = &aggregate.final_function {
        options.push(format!(
            "FINALFUNC = {}",
            qualified_iden(dialect, final_function)
        ));
    }
    if let Some(initial_condition) = &aggregate.initial_condition {
        options.push(format!(
            "INITCOND = {}",
            quote_sql_literal(initial_condition)
        ));
    }

    Ok(format!(
        "{}\nCREATE AGGREGATE {}({}) (\n    {}\n);",
        advanced_object_notice("aggregate"),
        qualified_name(dialect, &aggregate.name, &aggregate.schema),
        render_parameters(dialect, &aggregate.parameters),
        options.join(",\n    ")
    ))
}

fn render_trigger(dialect: &SqlDialect, trigger: &crate::schema::Trigger) -> Result<String> {
    let timing = trigger.timing.ok_or_else(|| {
        ShkiError::schema(format!(
            "Cannot render trigger '{}' without timing metadata",
            trigger.name
        ))
    })?;
    let orientation = trigger.orientation.ok_or_else(|| {
        ShkiError::schema(format!(
            "Cannot render trigger '{}' without orientation metadata",
            trigger.name
        ))
    })?;
    if trigger.events.is_empty() {
        return Err(ShkiError::schema(format!(
            "Cannot render trigger '{}' without event metadata",
            trigger.name
        )));
    }

    let timing = match timing {
        TriggerTiming::Before => "BEFORE",
        TriggerTiming::After => "AFTER",
        TriggerTiming::InsteadOf => "INSTEAD OF",
    };
    let mut events = trigger
        .events
        .iter()
        .map(|event| match event {
            TriggerEvent::Insert => "INSERT",
            TriggerEvent::Update => "UPDATE",
            TriggerEvent::Delete => "DELETE",
            TriggerEvent::Truncate => "TRUNCATE",
        })
        .collect::<Vec<_>>();
    events.sort_unstable();
    events.dedup();
    let orientation = match orientation {
        TriggerOrientation::Row => "ROW",
        TriggerOrientation::Statement => "STATEMENT",
    };

    Ok(format!(
        "{}\nCREATE TRIGGER {}\n{} {} ON {}\nFOR EACH {}\nEXECUTE FUNCTION {}();",
        advanced_object_notice("trigger"),
        quote_identifier(dialect, &trigger.name),
        timing,
        events.join(" OR "),
        qualified_iden(dialect, &trigger.table),
        orientation,
        qualified_iden(dialect, &trigger.function)
    ))
}

fn render_parameters(
    dialect: &SqlDialect,
    parameters: &[crate::schema::FunctionParameter],
) -> String {
    parameters
        .iter()
        .map(|parameter| {
            let mut parts = Vec::new();
            if let Some(mode) = parameter.mode {
                parts.push(
                    match mode {
                        FunctionParameterMode::In => "IN",
                        FunctionParameterMode::Out => "OUT",
                        FunctionParameterMode::InOut => "INOUT",
                        FunctionParameterMode::Variadic => "VARIADIC",
                    }
                    .to_string(),
                );
            }
            if let Some(name) = &parameter.name {
                parts.push(quote_identifier(dialect, name));
            }
            parts.push(parameter.data_type.to_string(dialect));
            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn qualified_iden(dialect: &SqlDialect, iden: &Iden) -> String {
    qualified_name(dialect, &iden.name, &iden.schema)
}

fn advanced_object_notice(object_kind: &str) -> String {
    format!("-- Rendered from currently represented Catalog fields for this {object_kind}.")
}

fn required_routine_language<'a>(
    kind: &str,
    signature: &str,
    language: Option<&'a str>,
) -> Result<&'a str> {
    language
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ShkiError::schema(format!(
                "Cannot render {kind} '{signature}' without language metadata"
            ))
        })
}

fn required_routine_body<'a>(
    kind: &str,
    signature: &str,
    body: Option<&'a str>,
) -> Result<&'a str> {
    body.filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ShkiError::schema(format!(
                "Cannot render {kind} '{signature}' without body metadata"
            ))
        })
}

fn dollar_quote_tag(body: &str) -> String {
    let mut index = 0;
    loop {
        let tag = if index == 0 {
            "shki".to_string()
        } else {
            format!("shki_{index}")
        };
        if !body.contains(&format!("${tag}$")) {
            return tag;
        }
        index += 1;
    }
}

fn quote_sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sanitize_file_name(value: &str) -> String {
    let mut sanitized = String::new();
    let mut last_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            sanitized.push('_');
            last_was_separator = true;
        }
    }

    sanitized.trim_matches('_').to_string()
}

fn ensure_trailing_newline(mut content: String) -> String {
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

pub fn render_snapshot(snapshot: &Snapshot, format: &SchemaExportFormat) -> Result<String> {
    match format {
        SchemaExportFormat::Json => snapshot.to_json(),
        SchemaExportFormat::Sql => {
            let empty = Snapshot::new(snapshot.dialect);
            let diff = diff_snapshots(&empty, snapshot)?;
            let generator = SqlRenderer::new(&snapshot.dialect);
            generator.generate_string(&diff.statements)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::iden::Iden;
    use crate::schema::{
        Column, ColumnPrivilege, DataType, DefaultPrivilege, FunctionParameter, Index,
        ObjectPrivilege, PartitionAttachment, RevokedDefaultPrivilege, RowLevelSecurity,
        RowLevelSecurityPolicy, SqlDialect, Table, Trigger,
    };

    #[test]
    fn directory_schema_preview_prints_file_summary_paths_and_contents() {
        let mut snapshot = Snapshot::new(SqlDialect::Postgres);
        let mut table = Table::in_schema("users", "public");
        table.column(Column::new("id", DataType::Integer).primary_key());
        snapshot.insert_table(Iden::new("users", Some("public".to_string())), table);

        let preview = render_directory_schema_preview(
            &Config {
                common: crate::CommonArgs {
                    no_color: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &snapshot,
        )
        .expect("Directory Schema preview should render");

        assert!(preview.contains("2 file(s):"));
        assert!(preview.contains("main.sql"));
        assert!(preview.contains("\\i public/tables/users.sql"));
        assert!(preview.contains("public/tables/users.sql"));
        assert!(preview.contains("CREATE TABLE \"public\".\"users\""));
    }

    #[test]
    fn directory_schema_files_render_postgres_catalog_entries() {
        let mut snapshot = Snapshot::new(SqlDialect::Postgres);
        let mut table = Table::in_schema("users", "public");
        table.column(Column::new("id", DataType::Integer).primary_key());
        table.column(Column::new("name", DataType::Text));
        table.index(Index::new("users_name_idx", vec!["name"]));
        snapshot.insert_table(Iden::new("users", Some("public".to_string())), table);

        let schema = snapshot.catalog.ensure_schema("public");
        schema.triggers.insert(
            "users_touch".to_string(),
            Trigger {
                name: "users_touch".to_string(),
                table: Iden::new("users", Some("public".to_string())),
                function: Iden::new("touch_user", Some("public".to_string())),
                events: vec![TriggerEvent::Insert],
                timing: Some(TriggerTiming::Before),
                orientation: Some(TriggerOrientation::Row),
            },
        );
        schema.row_level_security.insert(
            "users".to_string(),
            RowLevelSecurity {
                table: Iden::new("users", Some("public".to_string())),
                forced: true,
            },
        );
        schema.row_level_security_policies.insert(
            "users_policy".to_string(),
            RowLevelSecurityPolicy {
                name: "users_policy".to_string(),
                table: Iden::new("users", Some("public".to_string())),
                permissive: true,
                roles: vec!["app_user".to_string()],
                command: "SELECT".to_string(),
                using_expression: Some("id > 0".to_string()),
                check_expression: None,
            },
        );
        schema.partition_attachments.insert(
            "users_2026".to_string(),
            PartitionAttachment {
                parent: Iden::new("users_parent", Some("public".to_string())),
                child: Iden::new("users", Some("public".to_string())),
                bound: "FOR VALUES FROM (1) TO (100)".to_string(),
            },
        );
        schema.object_privileges.push(ObjectPrivilege {
            object_type: "BASE TABLE".to_string(),
            object: Iden::new("users", Some("public".to_string())),
            grantee: "app_user".to_string(),
            privilege_type: "SELECT".to_string(),
            grantable: false,
        });
        schema.column_privileges.push(ColumnPrivilege {
            table: Iden::new("users", Some("public".to_string())),
            column: "name".to_string(),
            grantee: "app_user".to_string(),
            privilege_type: "UPDATE".to_string(),
            grantable: true,
        });
        schema.default_privileges.push(DefaultPrivilege {
            owner_role: "postgres".to_string(),
            object_type: "TABLES".to_string(),
            grantee: "app_user".to_string(),
            privilege_type: "SELECT".to_string(),
            grantable: false,
        });
        schema
            .revoked_default_privileges
            .push(RevokedDefaultPrivilege {
                owner_role: "postgres".to_string(),
                object_type: "FUNCTIONS".to_string(),
                grantee: "PUBLIC".to_string(),
                privilege_type: "EXECUTE".to_string(),
            });

        let files = directory_schema_files(&snapshot).expect("Directory Schema should render");
        let main_sql = files
            .iter()
            .find(|file| file.path == *"main.sql")
            .expect("main.sql should exist");
        let table_sql = files
            .iter()
            .find(|file| file.path == *"public/tables/users.sql")
            .expect("table file should exist");

        assert!(
            table_sql
                .content
                .contains("CREATE INDEX \"users_name_idx\"")
        );
        assert!(table_sql.content.contains("CREATE TRIGGER \"users_touch\""));
        assert!(
            table_sql
                .content
                .contains("ALTER TABLE \"public\".\"users\" ENABLE ROW LEVEL SECURITY")
        );
        assert!(
            table_sql
                .content
                .contains("ALTER TABLE \"public\".\"users\" FORCE ROW LEVEL SECURITY")
        );
        assert!(table_sql.content.contains("CREATE POLICY \"users_policy\""));
        assert!(table_sql.content.contains("TO \"app_user\""));
        assert!(table_sql.content.contains(
            "ALTER TABLE \"public\".\"users_parent\" ATTACH PARTITION \"public\".\"users\" FOR VALUES FROM (1) TO (100)"
        ));
        assert!(
            table_sql
                .content
                .contains("GRANT SELECT ON TABLE \"public\".\"users\" TO \"app_user\"")
        );
        assert!(table_sql.content.contains(
            "GRANT UPDATE (\"name\") ON TABLE \"public\".\"users\" TO \"app_user\" WITH GRANT OPTION"
        ));
        assert!(
            main_sql
                .content
                .contains("\\i public/privileges/default.sql")
        );
        assert!(
            main_sql
                .content
                .contains("\\i public/privileges/revoked_default.sql")
        );
        assert!(
            !main_sql
                .content
                .contains("public/indexes/users_name_idx.sql")
        );
        assert!(!main_sql.content.contains("public/triggers/users_touch.sql"));

        let default_sql = files
            .iter()
            .find(|file| file.path == *"public/privileges/default.sql")
            .expect("default privileges file should exist");
        assert!(default_sql.content.contains(
            "ALTER DEFAULT PRIVILEGES FOR ROLE \"postgres\" IN SCHEMA \"public\" GRANT SELECT ON TABLES TO \"app_user\""
        ));

        let revoked_default_sql = files
            .iter()
            .find(|file| file.path == *"public/privileges/revoked_default.sql")
            .expect("revoked default privileges file should exist");
        assert!(revoked_default_sql.content.contains(
            "ALTER DEFAULT PRIVILEGES FOR ROLE \"postgres\" IN SCHEMA \"public\" REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC"
        ));
    }

    #[test]
    fn function_renderer_uses_safe_dollar_quote_tag_and_requires_metadata() {
        let function = Function {
            name: "normalize_name".to_string(),
            schema: Some("public".to_string()),
            signature: "normalize_name(text)".to_string(),
            parameters: vec![FunctionParameter::new(
                Some("value".to_string()),
                DataType::Text,
            )],
            return_type: Some(DataType::Text),
            language: Some("sql".to_string()),
            body: Some("SELECT '$shki$' || value".to_string()),
        };

        let sql = render_function(&SqlDialect::Postgres, &function)
            .expect("function should render with represented metadata");

        assert!(sql.contains("Rendered from currently represented Catalog fields"));
        assert!(sql.contains("AS $shki_1$"));
        assert!(sql.contains("LANGUAGE \"sql\""));
        assert!(sql.contains("RETURNS TEXT"));

        let missing_body = Function {
            body: None,
            ..function
        };
        let error = render_function(&SqlDialect::Postgres, &missing_body)
            .expect_err("function without a body should not render");
        assert!(error.to_string().contains("without body metadata"));
    }

    #[test]
    fn aggregate_renderer_requires_transition_function_and_escapes_initial_condition() {
        let aggregate = Aggregate {
            name: "first_value".to_string(),
            schema: Some("public".to_string()),
            signature: "first_value(text)".to_string(),
            parameters: vec![FunctionParameter::new(None, DataType::Text)],
            return_type: DataType::Text,
            state_type: DataType::Text,
            transition_function: Some(Iden::new("first_sfunc", Some("public".to_string()))),
            final_function: None,
            initial_condition: Some("can't".to_string()),
        };

        let sql = render_aggregate(&SqlDialect::Postgres, &aggregate)
            .expect("aggregate should render with required metadata");

        assert!(sql.contains("SFUNC = \"public\".\"first_sfunc\""));
        assert!(sql.contains("STYPE = TEXT"));
        assert!(sql.contains("INITCOND = 'can''t'"));

        let missing_transition = Aggregate {
            transition_function: None,
            ..aggregate
        };
        let error = render_aggregate(&SqlDialect::Postgres, &missing_transition)
            .expect_err("aggregate without transition function should not render");
        assert!(error.to_string().contains("without a transition function"));
    }

    #[test]
    fn trigger_renderer_orders_events_and_requires_complete_metadata() {
        let trigger = Trigger {
            name: "users_touch".to_string(),
            table: Iden::new("users", Some("public".to_string())),
            function: Iden::new("touch_user", Some("public".to_string())),
            events: vec![
                TriggerEvent::Update,
                TriggerEvent::Insert,
                TriggerEvent::Update,
            ],
            timing: Some(TriggerTiming::Before),
            orientation: Some(TriggerOrientation::Row),
        };

        let sql = render_trigger(&SqlDialect::Postgres, &trigger)
            .expect("trigger should render with complete metadata");

        assert!(sql.contains("BEFORE INSERT OR UPDATE ON"));
        assert!(sql.contains("FOR EACH ROW"));
        assert!(sql.contains("EXECUTE FUNCTION \"public\".\"touch_user\"()"));

        let missing_events = Trigger {
            events: Vec::new(),
            ..trigger
        };
        let error = render_trigger(&SqlDialect::Postgres, &missing_events)
            .expect_err("trigger without events should not render");
        assert!(error.to_string().contains("without event metadata"));
    }
}
