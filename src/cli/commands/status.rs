use std::collections::HashSet;
use std::time::Duration;

use crate::MigrationManager;
use crate::Result;
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
}

pub async fn display_migrations(manager: &MigrationManager, config: &Config) -> Result<()> {
    let all_migrations = manager.list_migrations()?;
    if all_migrations.is_empty() {
        println!("{}", "No migrations found".yellow());
        return Ok(());
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

    let applied_set: std::collections::HashSet<String> = if let Some(a) = applied.as_ref() {
        a.iter().map(|m| m.name.clone()).collect()
    } else {
        HashSet::new()
    };

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

            MigrationState {
                status: status.to_string(),
                name: name.bright_white().to_string(),
                down: if has_down {
                    format!(" {}", DOWN_SYMBOL.cyan())
                } else {
                    " x".red().to_string()
                },
                applied_at: if let Some(a) = applied.as_ref() {
                    a.iter()
                        .find(|m| m.name == name)
                        .map(|m| m.applied_at.clone())
                        .unwrap_or_default()
                } else {
                    "".to_string()
                },
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

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("URL {}", url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table);

    display_migrations(&migration_manager, config).await
}
