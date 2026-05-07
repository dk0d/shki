pub mod checksum;
pub mod manager;
pub mod queries;
pub mod utils;

use crate::config::Config;

use crate::display::tables::display_migrations;
use crate::pool::create_any_pool_opts;
// use super::introspect::introspect_db;
// use crate::checksum::sql_checksum;
use crate::{Result, ShkiError};
use colored::Colorize;

// use crate::cli::commands::status::display_migrations;

use sqlx::AnyPool;

use self::manager::MigrationManager;

pub async fn cmd_migrate(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    }

    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let pool: AnyPool = create_any_pool_opts()
        .max_connections(2)
        .connect(db_url)
        .await?;

    let manager = MigrationManager::from_config(config);

    // migration_manager.validate_snapshots()?;
    manager.validate_checksums(&pool).await?;
    // migration_manager.ensure_snapshot_coverage(&pool).await?;

    let pending = manager.get_pending_migrations(&pool).await?;
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

        let _checksum = manager.apply_migration(&pool, &migration_path).await?;

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
