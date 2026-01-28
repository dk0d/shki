use crate::{Config, MigrationManager, Result, ShkiError, Snapshot, SqlGenerator, diff_snapshots};
use colored::Colorize;
use std::path::PathBuf;

/// Load a schema snapshot from a file path
///
/// Supports:
/// - `.lua` files (requires `lua` feature)
/// - `.json` files (snapshot format)
fn load_snapshot_from_path(path: &PathBuf, _config: &Config) -> Result<Snapshot> {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match extension {
        "lua" => {
            println!("{} {}", "Loading Lua schema:".cyan(), path.display());
            let schema = crate::lua::load_schema_from_file(path)?;
            Ok(Snapshot::from_schema(&schema))
        }
        "json" => {
            let content = std::fs::read_to_string(path)?;
            Snapshot::from_json(&content)
        }
        _ => Err(ShkiError::config(format!(
            "Unsupported schema file extension: '{}'. Supported: .lua, .json",
            extension
        ))),
    }
}

/// Load schema from config.schema glob patterns
///
/// This function resolves the glob patterns in config.schema and loads/merges
/// all matching schema files into a single Snapshot.
fn load_snapshot_from_config(config: &Config) -> Result<Snapshot> {
    if config.schema.is_empty() {
        return Err(ShkiError::config(
            "No schema files found. Either:\n  \
                     - Provide a schema path with --schema <path>\n  \
                     - Configure schema patterns in shki.toml under 'schema'",
        ));
    }
    let path = PathBuf::from(&config.schema);
    let schema = crate::lua::load_schema_from_file(&path)?;
    Ok(Snapshot::from_schema(&schema))
}

pub fn cmd_generate_sql(
    config: &Config,
    name: Option<String>,
    schema_path: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "Loading schema definitions...".cyan());
    let desired_snapshot = if let Some(path) = schema_path {
        load_snapshot_from_path(&path, config)?
    } else {
        load_snapshot_from_config(config)?
    };

    // Load previous snapshot from migrations/_meta/
    let migration_manager = MigrationManager::new(&config.out, config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    let prev_snapshot = migration_manager.load_latest_snapshot()?;

    // Determine the base snapshot for diffing:
    // - If we have a previous snapshot, diff against it
    // - If no previous migrations exist, diff against empty snapshot (initial migration)
    let base_snapshot = prev_snapshot
        .clone()
        .unwrap_or_else(|| Snapshot::new(config.dialect));

    let is_initial = prev_snapshot.is_none();
    if is_initial {
        println!(
            "{}",
            "No existing migrations found. Creating initial migration...".yellow()
        );
    } else {
        println!("  {} previous migration snapshot", "Found".green());
    }

    // Compute diff (from previous/empty snapshot to desired schema)
    let diff = diff_snapshots(&base_snapshot, &desired_snapshot)?;

    if diff.is_empty() {
        println!("{}", "No changes detected".green());
        return Ok(());
    }

    // Generate SQL
    let generator = SqlGenerator::new(config.dialect).with_breakpoints(config.breakpoints);
    let sql = generator.generate_sql(&diff)?;

    println!("\n{}", "Changes detected:".yellow());
    println!("{}", diff.summary());

    if dry_run {
        println!("\n{}", "SQL (dry run):".cyan());
        println!("{}", sql);
        return Ok(());
    }

    // Create migration file (store the desired schema as the new snapshot)
    let migration_path = migration_manager.create_migration(
        name,
        &sql,
        prev_snapshot.as_ref(),
        &desired_snapshot,
    )?;

    println!("\n{}", "Migration created:".green());
    println!("  {}", migration_path.display());

    Ok(())
}
