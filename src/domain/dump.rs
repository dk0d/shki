use crate::diff::diff_snapshots;
use crate::engines::Engine;
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::generator::SqlGenerator;
use crate::utils::resolve_path;
use crate::{Config, Result};

use colored::Colorize;
use serde::Serialize;

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
    schema: &Option<String>,
) -> Result<()> {
    export_live_schema(config, format, output, schema, "Dump").await
}

pub async fn export_live_schema(
    config: &Config,
    format: &SchemaExportFormat,
    output: Option<&std::path::Path>,
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
    let content = render_snapshot(&snapshot, format)?;

    match output {
        Some(path) => {
            let resolved_path = resolve_path(None, path);
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
