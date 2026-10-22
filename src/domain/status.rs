use crate::Result;
use crate::config::Config;

use super::display::tables::display_migrations;
use super::migrate::manager::MigrationManager;

/// Show migration status
pub async fn cmd_status(config: &Config) -> Result<()> {
    config.display_sanitized_db_url();

    let migration_manager = MigrationManager::from_config(config).await?;

    let status = migration_manager.migration_status().await?;
    display_migrations(&status);
    migration_manager.validate_migration_status(&status)
}
