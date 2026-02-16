use crate::{Config, MigrationManager, Result, Snapshot, SqlGenerator, diff_snapshots};
use colored::Colorize;
use std::path::PathBuf;

pub fn cmd_generate_sql(
    config: &Config,
    name: Option<String>,
    schema_path: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "Loading schema definitions...".cyan());
    let desired_snapshot = if let Some(path) = schema_path {
        // Resolve the schema path relative to the project root
        let resolved_path = config.resolve_path(&path);
        Snapshot::from_path(&resolved_path)?
    } else {
        Snapshot::from_config(config)?
    };

    // Load previous snapshot from migrations/_meta/
    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect)
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

    // Generate down SQL if configured
    let down_sql = if config.migrations.generate_down {
        let (down_diff, irreversible) = diff.get_down_diff();

        if !irreversible.is_empty() {
            println!(
                "{} {} statement(s) cannot be automatically reversed:",
                "Warning:".yellow(),
                irreversible.len()
            );
            for stmt in &irreversible {
                println!("  - {:?}", std::mem::discriminant(stmt));
            }
            println!(
                "  {}",
                "These will need manual intervention in the down migration.".yellow()
            );
        }

        if !down_diff.is_empty() {
            Some(generator.generate_sql(&down_diff)?)
        } else {
            None
        }
    } else {
        None
    };

    println!("\n{}", "Changes detected:".yellow());
    println!("{}", diff.summary());

    if dry_run {
        println!("\n{}", "Up migration SQL (dry run):".cyan());
        println!("{}", sql);

        if let Some(ref down) = down_sql {
            println!("\n{}", "Down migration SQL (dry run):".cyan());
            println!("{}", down);
        }
        return Ok(());
    }

    // Create migration file (store the desired schema as the new snapshot)
    let (up_path, down_path) = migration_manager.create_migration_with_down(
        name,
        &sql,
        down_sql.as_deref(),
        prev_snapshot.as_ref(),
        &desired_snapshot,
    )?;

    println!("\n{}", "Migration created:".green());
    println!("  Up:   {}", up_path.display());
    if let Some(down) = down_path {
        println!("  Down: {}", down.display());
    }

    Ok(())
}
