pub mod checksum;
pub mod manager;
pub mod utils;

use crate::config::Config;

use crate::display::tables::display_migrations;
// use super::introspect::introspect_db;
// use crate::checksum::sql_checksum;
use crate::{Result, ShkiError};
use colored::Colorize;

use self::manager::MigrationManager;

pub async fn cmd_migrate(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    }

    config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let manager = MigrationManager::from_config(config).await?;

    // migration_manager.validate_snapshots()?;
    manager.validate_checksums().await?;
    // migration_manager.ensure_snapshot_coverage(&pool).await?;

    let pending = manager.get_pending_migrations().await?;
    let mut applied = Vec::with_capacity(pending.len());

    // if dry_run {
    //     display_migrations(&manager, config).await?;
    //     println!("{}", "(dry run - no changes applied)".cyan());
    //     return Ok(());
    // }

    for migration_path in pending {
        let name = migration_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?
            .to_string();

        let _checksum = manager.apply_migration(&migration_path).await?;

        // let mut snapshot = introspect_db(config).await?;
        // snapshot.tables.shift_remove(&config.migrations.table);
        // migration_manager.save_post_migration_snapshot(snapshot, &name, &checksum)?;

        applied.push(name);
    }

    println!(
        "{} migration(s) applied\n\n",
        applied.len().to_string().green()
    );

    display_migrations(&manager, config).await?;

    Ok(())
}
