use crate::config::Config;

use crate::{MigrationManager, Result, ShkiError};
use colored::Colorize;
use sqlx::any::AnyPoolOptions;

pub async fn cmd_migrate(config: &Config, dry_run: bool) -> Result<()> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    sqlx::any::install_default_drivers();

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await?;

    let migration_manager = MigrationManager::new(&config.out, config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    let pending = migration_manager.get_pending_migrations(&pool).await?;

    if pending.is_empty() {
        println!("\n{}\n", "No pending migrations".green());
        return Ok(());
    }

    display_pending_migrations(&pending);

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    let applied = migration_manager.apply_all(&pool).await?;

    display_applied_migrations(&applied);

    Ok(())
}

fn display_pending_migrations(pending: &[std::path::PathBuf]) {
    println!(
        "\n\n{} pending migration(s):",
        pending.len().to_string().yellow()
    );

    for path in pending {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        println!("  - {}", name);
    }
}

fn display_applied_migrations(applied: &[String]) {
    println!(
        "\n\n{} migration(s) applied:",
        applied.len().to_string().green()
    );

    for name in applied {
        println!("  - {}", name);
    }
}
