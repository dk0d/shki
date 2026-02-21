use super::introspect::introspect_db;
use crate::{Config, MigrationManager, Snapshot, diff::diff_snapshots};

use colored::Colorize;

use crate::{Result, SqlGenerator, load_schema_from_file};
/// Show the diff between schema and database
pub async fn cmd_diff(
    config: &Config,
    schema_path: Option<&std::path::Path>,
    show_sql: bool,
) -> Result<()> {
    println!("{}", "Introspecting database...".yellow());

    let db_snapshot = introspect_db(config).await?;

    println!(
        "URL: {}",
        config
            .database_url
            .clone()
            .unwrap_or_default()
            .bright_green()
    );

    // Load desired schema from file if provided, otherwise use latest snapshot
    let desired_snapshot = if let Some(path) = schema_path {
        // Resolve the schema path relative to the project root
        let resolved_path = config.resolve_path(path);
        load_schema_from_file(&resolved_path)?.into()
    } else {
        let migration_manager = MigrationManager::new(config.out_dir(), config.dialect);
        migration_manager
            .load_latest_snapshot()?
            .unwrap_or_else(|| Snapshot::new(config.dialect))
    };

    let diff = diff_snapshots(&db_snapshot, &desired_snapshot)?;

    if diff.is_empty() {
        println!("{}", "No differences found".green());
        return Ok(());
    }

    println!("\n{}", "Differences:".yellow());
    println!("{}", diff.summary());

    if show_sql {
        let generator = SqlGenerator::new(config.dialect);
        let sql = generator.generate_sql(&diff)?;
        println!("\n{}", "SQL:".cyan());
        println!("{}", sql);
    }

    Ok(())
}
