use crate::Result;
use crate::config::Config;
use colored::Colorize;

use super::display::tables::display_migrations;
use super::migrate::manager::MigrationManager;

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }
    let migration_manager = MigrationManager::from_config(config).await?;

    display_migrations(&migration_manager, config).await?;

    // Perform validation checks
    let mut has_errors = false;
    let has_warnings = false;

    // Validate applied migration checksums if database is available
    if config.database_url.is_some()
        && let Err(e) = migration_manager.validate_checksums().await
    {
        println!("{}", "Checksum Validation Failed".red().bold());
        println!("{}", e);
        has_errors = true;
    }

    if has_errors {
        println!(
            "{}",
            "Validation errors found. Please resolve before running migrations.".red()
        );
    } else if has_warnings {
        println!("{}", "Warnings found. Review the issues above.".yellow());
    }

    Ok(())
}
