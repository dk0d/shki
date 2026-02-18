use colored::Colorize;
use dialoguer::theme::ColorfulTheme;

use crate::{Config, MigrationManager, Result, ShkiError, create_any_pool};

// Rollback migrations using down migration files
pub async fn cmd_down(config: &Config, count: Option<usize>, dry_run: bool) -> Result<()> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let pool = create_any_pool(db_url).await;

    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    // Get migrations that can be rolled back
    let rollback_migrations = migration_manager.get_rollback_migrations(&pool).await?;

    if rollback_migrations.is_empty() {
        println!(
            "\n\n{} {}",
            "No migrations to rollback".yellow(),
            "(no down migration files found for applied migrations)".dimmed()
        );
        return Ok(());
    }

    // Determine how many to rollback
    let to_rollback: Vec<_> = match count {
        Some(n) => rollback_migrations.into_iter().take(n).collect(),
        None => rollback_migrations.into_iter().take(1).collect(),
    };

    println!(
        "{} migration(s) to rollback:",
        to_rollback.len().to_string().yellow()
    );

    for path in &to_rollback {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".down.sql"))
            .unwrap_or("unknown");
        println!("  - {}", name);
    }

    if dry_run {
        println!("\n{}", "(dry run - no changes applied)".cyan());
        return Ok(());
    }

    // Confirm rollback
    println!();
    let confirmed = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to rollback these migrations?")
        .default(false)
        .interact_opt()
        .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?
        .unwrap_or(false);

    if !confirmed {
        println!("{}", "Aborted".yellow());
        return Ok(());
    }

    // Perform rollback
    println!();

    let mut rolled_back = Vec::new();

    for down_path in to_rollback {
        let name = down_path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".down.sql"))
            .unwrap_or("unknown")
            .to_string();

        if config.verbose {
            println!("Rolling back: {}", name);
        }

        migration_manager
            .rollback_migration(&pool, &down_path)
            .await?;
        rolled_back.push(name);
    }

    println!(
        "\n{} migration(s) rolled back:",
        rolled_back.len().to_string().green(),
    );
    for name in &rolled_back {
        println!("  - {}", name);
    }

    Ok(())
}
