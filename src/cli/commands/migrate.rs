use crate::config::Config;

use crate::{MigrationManager, Result, ShkiError};
use colored::Colorize;

pub async fn cmd_migrate(config: &Config, dry_run: bool) -> Result<()> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let pool = sqlx::AnyPool::connect(db_url).await?;

    let migration_manager = MigrationManager::new(&config.out, config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    let pending = migration_manager.get_pending_migrations(&pool).await?;

    if pending.is_empty() {
        println!("\n{}\n", "No pending migrations".green());
        return Ok(());
    }

    println!(
        "{} pending migration(s):",
        pending.len().to_string().yellow()
    );
    for path in &pending {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        println!("  - {}", name);
    }

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    println!();
    let applied = migration_manager.apply_all(&pool).await?;

    println!(
        "{} migration(s) applied:",
        applied.len().to_string().green()
    );
    for name in &applied {
        println!("  - {}", name);
    }

    Ok(())
}
