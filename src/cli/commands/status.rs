use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::MigrationManager;
use crate::MigrationRow;
use crate::Result;
use crate::Snapshot;
use crate::config::Config;
use colored::Colorize;
use tabled::{
    Table, Tabled,
    settings::{Color, Style, object::Columns},
};

use crate::create_any_pool_opts;

const DOWN_SYMBOL: &str = "↓";

#[derive(Debug, Tabled)]
pub struct MigrationState {
    status: String,
    name: String,
    down: String,
    applied_at: String,
    checksum: String,
}

pub async fn display_migrations(manager: &MigrationManager, config: &Config) -> Result<()> {
    let all_migrations = manager.list_migrations()?;

    if all_migrations.is_empty() {
        println!("{}", "No migrations found".yellow());
        return Ok(());
    }

    let snapshots = Snapshot::load_all(&config.out_dir())?;

    if snapshots.is_empty() {
        println!("{}", "No snapshots found".red());
    }

    // Try to get applied migrations if database URL is available
    let applied = if let Some(db_url) = config.database_url.as_ref() {
        let pool = create_any_pool_opts()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(config.timeout_seconds))
            .connect(db_url)
            .await?;
        let migrations = manager.get_applied_migrations(&pool).await?;
        Some(migrations)
    } else {
        None
    };

    let applied_set: HashSet<&str> = applied
        .as_deref()
        .map(|rows| rows.iter().map(|m| m.name.as_str()).collect())
        .unwrap_or_default();

    let applied_by_name: HashMap<&str, &MigrationRow> = applied
        .as_deref()
        .map(|rows| rows.iter().map(|m| (m.name.as_str(), m)).collect())
        .unwrap_or_default();

    println!("\n{}\n", "Migration Status".cyan());

    let migrations = all_migrations
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            let status = if applied_set.contains(name) {
                "applied".green()
            } else {
                "pending".yellow()
            };

            // Check if down migration exists
            let has_down = manager.has_down_migration(path);

            let snapshot = snapshots
                .iter()
                .find(|s| s.migration.as_ref().is_some_and(|m| m.name == name));
            let snapshot_checksum = if let Some(s) = snapshot
                && let Some(m) = &s.migration
            {
                Some(m.checksum.clone())
            } else {
                None
            };

            let checksum = applied_by_name
                .get(name)
                .and_then(|m| m.checksum.as_deref())
                .map(|c| format!("{}...", c.get(..5).unwrap_or(c)))
                .unwrap_or_else(|| format!("{}...", &snapshot_checksum.unwrap_or_default()[..5]));

            MigrationState {
                status: status.to_string(),
                name: name.bright_white().to_string(),
                checksum,
                down: if has_down {
                    format!(" {}", DOWN_SYMBOL.cyan())
                } else {
                    " x".red().to_string()
                },
                applied_at: applied_by_name
                    .get(name)
                    .map(|m| m.applied_at.clone())
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();

    let mut table = Table::new(&migrations);
    table
        .with(Style::psql())
        .modify(Columns::new(0..), Color::FG_BLUE);

    println!("{}", table);

    // TODO: use verbose flag to show these types of things
    // Show legend
    println!();
    println!("  {} = down migration available", DOWN_SYMBOL.cyan());
    println!("  {} = down migration not available", "x".red());

    Ok(())
}

pub fn display_migration_rows(migrations: &[MigrationRow]) {
    let mut table = Table::new(migrations);
    table
        .with(Style::psql())
        .modify(Columns::new(0..), Color::FG_BLUE);

    println!("{}", table);
}

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("URL {}", url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table);

    let migration_manager = if let Some(schema) = &config.migrations.schema {
        migration_manager.with_table_schema(schema)
    } else {
        migration_manager
    };

    display_migrations(&migration_manager, config).await?;

    // Perform validation checks
    let mut has_errors = false;
    let mut has_warnings = false;

    // Validate snapshots against migration files
    if let Err(e) = migration_manager.validate_snapshots() {
        println!();
        println!("{}", "Snapshot Validation Failed".red().bold());
        println!("{}", e);
        has_errors = true;
    }

    // Validate applied migration checksums if database is available
    if let Some(db_url) = config.database_url.as_ref() {
        let pool = create_any_pool_opts()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(config.timeout_seconds))
            .connect(db_url)
            .await?;

        if let Err(e) = migration_manager.validate_checksums(&pool).await {
            println!();
            println!("{}", "Checksum Validation Failed".red().bold());
            println!("{}", e);
            has_errors = true;
        }

        // Check for migrations without snapshots
        let (applied, missing_snapshots, checksums_match) = migration_manager
            .find_migrations_without_snapshots(&pool)
            .await?;

        if !missing_snapshots.is_empty() {
            println!();
            println!("------\n{}", "Found missing snapshots".bright_red().bold());
            println!("{}", "Database State".yellow());

            display_migration_rows(&applied);

            println!();
            println!(
                "{}: Applied migrations without snapshots",
                "Warning".yellow().bold()
            );
            println!(
                "The following migrations exist in the database but don't have corresponding snapshots:"
            );
            for row in &missing_snapshots {
                println!(
                    "  - {} ({})",
                    row.name.yellow(),
                    &row.checksum.clone().unwrap_or_default()[..5].dimmed()
                );
            }
            println!();

            if !checksums_match.is_empty() {
                println!();
                println!(
                    "{}: There are migrations with mismatched names but matching checksums",
                    "NOTE".bright_blue().bold()
                );
                for (migration_name, snapshot_name) in &checksums_match {
                    println!(
                        "  - {} ({})",
                        migration_name.yellow(),
                        snapshot_name.dimmed()
                    );
                }
            }
            has_warnings = true;
        }
    }

    if has_errors {
        println!();
        println!(
            "{}",
            "Validation errors found. Please resolve before running migrations.".red()
        );
    } else if has_warnings {
        println!();
        println!("{}", "Warnings found. Review the issues above.".yellow());
    }

    Ok(())
}
