pub mod checksum;
pub mod directives;
pub mod journal;
pub mod manager;
pub mod utils;

use crate::config::Config;

use crate::Result;
use crate::display::tables::display_migrations;
use colored::Colorize;

use self::manager::{ApplyMigrationMode, MigrationManager};

pub async fn cmd_migrate(
    config: &Config,
    mode: Option<ApplyMigrationMode>,
    dry_run: bool,
) -> Result<()> {
    config.require_database_url()?;

    let manager = MigrationManager::from_config(config).await?;

    if dry_run {
        display_migrations(&manager, config).await?;
        println!("{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    let applied = manager.apply(mode.unwrap_or_default()).await?;

    println!(
        "{} migration(s) applied\n\n",
        applied.len().to_string().green()
    );

    display_migrations(&manager, config).await?;

    Ok(())
}
