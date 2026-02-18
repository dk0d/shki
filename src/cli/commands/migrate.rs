use crate::config::Config;

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

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    let applied = migration_manager.apply_all(&pool).await?;

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
