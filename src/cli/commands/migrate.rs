use crate::config::Config;

use crate::{MigrationManager, Result, ShkiError, create_any_pool};
use colored::Colorize;

use crate::cli::commands::status::display_migrations;
use sqlx::AnyPool;

pub async fn cmd_migrate(config: &Config, dry_run: bool) -> Result<()> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    sqlx::any::install_default_drivers();

    let pool: AnyPool = create_any_pool(db_url).await;

    let migration_manager = MigrationManager::new(&config.out, config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    display_migrations(&migration_manager, config).await?;

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    let applied = migration_manager.apply_all(&pool).await?;

    println!(
        "\n\n{} migration(s) applied\n\n",
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
