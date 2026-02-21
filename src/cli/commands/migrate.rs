use crate::config::Config;

use super::introspect::introspect_db;
use crate::checksum::sql_checksum;
use crate::{MigrationManager, Result, ShkiError, create_any_pool_opts};
use colored::Colorize;

use crate::cli::commands::status::display_migrations;
use sqlx::AnyPool;

pub async fn cmd_migrate(config: &Config, dry_run: bool) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\nURL {}", url.bright_green());
    }

    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let pool: AnyPool = create_any_pool_opts()
        .max_connections(2)
        .connect(db_url)
        .await?;

    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    let migration_manager = if let Some(schema) = &config.migrations.schema {
        migration_manager.with_table_schema(schema)
    } else {
        migration_manager
    };

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    migration_manager.validate_snapshots()?;
    migration_manager.validate_checksums(&pool).await?;
    migration_manager.ensure_snapshot_coverage(&pool).await?;

    let pending = migration_manager.get_pending_migrations(&pool).await?;
    let mut applied = Vec::with_capacity(pending.len());

    for migration_path in pending {
        let name = migration_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?
            .to_string();

        migration_manager
            .apply_migration(&pool, &migration_path)
            .await?;

        let mut snapshot = introspect_db(config).await?;
        snapshot.tables.shift_remove(&config.migrations.table);

        let sql = std::fs::read_to_string(&migration_path)?;
        let checksum = sql_checksum(&sql);
        migration_manager.save_post_migration_snapshot(snapshot, &name, &checksum)?;

        applied.push(name);
    }

    println!(
        "{} migration(s) applied\n\n",
        applied.len().to_string().green()
    );

    display_migrations(&migration_manager, config).await?;

    Ok(())
}

// fn display_applied_migrations(applied: &[String]) {
//     let mut table = tabled::Table::new(applied.iter().map(|name| ("applied", name)));
//     table.with(tabled::settings::Style::psql());
//     println!("{}", table);
// }
