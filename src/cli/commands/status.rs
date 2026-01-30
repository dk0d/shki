use std::time::Duration;

use crate::MigrationManager;
use crate::Result;
use crate::config::Config;
use crate::create_any_pool_opts;
use colored::Colorize;
use tabled::Tabled;
use tabled::settings::Color;
use tabled::settings::object::Columns;
use tabled::{Table, settings::Style};

#[derive(Debug, Tabled)]
struct Migration {
    status: String,
    name: String,
    down: String,
}

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    let migration_manager = MigrationManager::new(&config.out, config.dialect)
        .with_table_name(&config.migrations.table);

    let all_migrations = migration_manager.list_migrations()?;

    if all_migrations.is_empty() {
        println!("{}", "No migrations found".yellow());
        return Ok(());
    }

    // Try to get applied migrations if database URL is available
    let applied = if let Some(db_url) = config.database_url.as_ref() {
        let pool = create_any_pool_opts()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(config.timeout_seconds))
            .connect(db_url)
            .await?;
        migration_manager.get_applied_migrations(&pool).await.ok()
    } else {
        None
    };

    let applied_set: std::collections::HashSet<String> =
        applied.unwrap_or_default().into_iter().collect();

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
            let has_down = migration_manager.has_down_migration(path);

            // println!("  [{}] {}{}", status, name, down_indicator.cyan());
            Migration {
                status: status.to_string(),
                name: name.bright_white().to_string(),
                down: if has_down {
                    " ↓".cyan().to_string()
                } else {
                    " x".red().to_string()
                },
            }
        })
        .collect::<Vec<_>>();

    let mut table = Table::new(&migrations);
    table
        .with(Style::psql())
        .modify(Columns::new(0..), Color::FG_BLUE);

    println!("{}", table);

    // Show legend
    println!();
    println!("  {} = down migration available", "↓".cyan());
    println!("  {} = down migration not available", "x".red());

    Ok(())
}
