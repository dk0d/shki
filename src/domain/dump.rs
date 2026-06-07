use crate::diff::diff_snapshots;
use crate::engines::Engine;
use crate::models::iden::Iden;
use crate::schema::{
    Aggregate, CatalogSchema, CompositeType, Domain, Function, FunctionParameterMode, Procedure,
    SqlDialect, TriggerEvent, TriggerOrientation, TriggerTiming,
};
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::generator::SqlGenerator;
use crate::sql::statements::{
    create_enum, create_extension, create_index, create_sequence, create_table, create_view,
    qualified_name, quote_identifier,
};
use crate::utils::resolve_path;
use crate::{Config, Result, ShkiError};

use colored::Colorize;
use serde::Serialize;
use std::path::{Path, PathBuf};

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
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

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
    let files = directory_schema_files(snapshot)?;
    let mut output = format!(
        "Directory Schema would create {} files:\n",
        files.len().to_string().cyan()
    );

    for file in files {
        let (name, content) = if config.no_color {
            (format!("-- {}", file.path.to_string_lossy()), file.content)
        } else {
            let content = {
                let mut buffer = String::new();
                let res = bat::PrettyPrinter::new()
                    .input_from_bytes(file.content.as_bytes())
                    .language("sql")
                    .print_with_writer(Some(&mut buffer));
                match res {
                    Ok(ok) => {
                        if ok {
                            buffer
                        } else {
                            file.content
                        }
                    }
                    Err(_) => file.content,
                }
            };

            (
                format!("{} {}", "--".dimmed(), file.path.to_string_lossy().dimmed()),
                content,
            )
        };
        output.push_str(&format!("\n{}\n{}", name, content));
    }

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
        .map(|path| format!("\\i {}", path.to_string_lossy()))
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
            create_table(&snapshot.dialect, table).to_string(None),
        );

        for index in table.indexes.values() {
            push_file(
                files,
                includes,
                schema_root
                    .join("indexes")
                    .join(format!("{}.sql", sanitize_file_name(&index.name))),
                create_index(
                    &snapshot.dialect,
                    &table.name,
                    &table.schema,
                    index,
                    false,
                    false,
                )
                .to_string(),
            );
        }
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

    for trigger in schema.triggers.values() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::iden::Iden;
    use crate::schema::{Column, DataType, FunctionParameter, SqlDialect, Table, Trigger};

    #[test]
    fn directory_schema_preview_prints_file_summary_paths_and_contents() {
        let mut snapshot = Snapshot::new(SqlDialect::Postgres);
        let mut table = Table::in_schema("users", "public");
        table.column(Column::new("id", DataType::Integer).primary_key());
        snapshot.insert_table(Iden::new("users", Some("public".to_string())), table);

        let preview = render_directory_schema_preview(
            &Config {
                no_color: true,
                ..Default::default()
            },
            &snapshot,
        )
        .expect("Directory Schema preview should render");

        assert!(preview.contains("Directory Schema would create 2 files:"));
        assert!(preview.contains("-- main.sql"));
        assert!(preview.contains("\\i public/tables/users.sql"));
        assert!(preview.contains("-- public/tables/users.sql"));
        assert!(preview.contains("CREATE TABLE \"public\".\"users\""));
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

pub fn render_snapshot(snapshot: &Snapshot, format: &SchemaExportFormat) -> Result<String> {
    match format {
        SchemaExportFormat::Json => snapshot.to_json(),
        SchemaExportFormat::Sql => {
            let empty = Snapshot::new(snapshot.dialect);
            let diff = diff_snapshots(&empty, snapshot)?;
            let generator = SqlGenerator::new(&snapshot.dialect);
            generator.generate_string(&diff.statements)
        }
    }
}
