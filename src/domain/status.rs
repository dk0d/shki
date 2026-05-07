use std::time::Duration;

use crate::Result;
use crate::config::Config;
use colored::Colorize;

use super::display::tables::display_migrations;
use super::migrate::manager::MigrationManager;
use super::pool::create_any_pool_opts;

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }
    let migration_manager = MigrationManager::from_config(config);

    display_migrations(&migration_manager, config).await?;

    // Perform validation checks
    let mut has_errors = false;
    let has_warnings = false;

    // Validate snapshots against migration files
    // if let Err(e) = migration_manager.validate_snapshots() {
    //     println!("{}", "Snapshot Validation Failed".red().bold());
    //     println!("{}", e);
    //     has_errors = true;
    // }

    // Validate applied migration checksums if database is available
    if let Some(db_url) = config.database_url.as_ref() {
        let pool = create_any_pool_opts()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(config.timeout_seconds))
            .connect(db_url)
            .await?;

        if let Err(e) = migration_manager.validate_checksums(&pool).await {
            println!("{}", "Checksum Validation Failed".red().bold());
            println!("{}", e);
            has_errors = true;
        }

        // Check for migrations without snapshots
        // let (applied, missing_snapshots, checksums_match) = migration_manager
        //     .find_migrations_without_snapshots(&pool)
        //     .await?;

        // if !missing_snapshots.is_empty() {
        //     println!("------\n{}", "Found missing snapshots".bright_red().bold());
        //     println!("{}", "Database State".yellow());
        //     display_migration_rows(&applied);
        //     println!(
        //         "{}: Applied migrations without snapshots",
        //         "Warning".yellow().bold()
        //     );
        //     println!(
        //         "The following migrations exist in the database but don't have corresponding snapshots:"
        //     );
        //     for row in &missing_snapshots {
        //         println!(
        //             "  - {} ({})",
        //             row.name.yellow(),
        //             &row.checksum.clone().unwrap_or_default()[..5].dimmed()
        //         );
        //     }
        //     if !checksums_match.is_empty() {
        //         println!(
        //             "{}: There are migrations with mismatched names but matching checksums",
        //             "NOTE".bright_blue().bold()
        //         );
        //         for (migration_name, snapshot_name) in &checksums_match {
        //             println!(
        //                 "  - {} ({})",
        //                 migration_name.yellow(),
        //                 snapshot_name.dimmed()
        //             );
        //         }
        //     }
        //     has_warnings = true;
        // }
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
