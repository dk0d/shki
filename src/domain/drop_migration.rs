use std::collections::HashSet;
use std::path::PathBuf;

use colored::Colorize;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect};

use crate::migrate::journal::JournalEntry;
use crate::{Config, Result, ShkiError};

use super::migrate::manager::MigrationManager;

#[derive(Debug, Clone)]
struct LocalMigration {
    name: String,
    up_path: PathBuf,
    down_path: PathBuf,
    applied: bool,
    journal_entry: Option<JournalEntry>,
}

/// Drop a migration file
pub async fn cmd_drop(config: &Config, migration: &Option<String>) -> Result<()> {
    let manager = MigrationManager::from_config(config).await?;
    let migrations = local_migrations(&manager).await?;

    if migrations.is_empty() {
        println!("\n{}", "No migrations found".yellow());
        return Ok(());
    }

    let Some(to_drop) = select_migration(&migrations, migration)? else {
        return Ok(());
    };

    if to_drop.applied && !confirm_drop(&to_drop.name)? {
        println!("{}", "Aborted".yellow());
        return Ok(());
    }

    drop_migration(&manager, to_drop)?;
    Ok(())
}

async fn local_migrations(manager: &MigrationManager) -> Result<Vec<LocalMigration>> {
    let applied = manager
        .get_applied_migrations()
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect::<HashSet<_>>();
    let journal = manager.load_journal()?;

    let mut migrations = manager
        .list_up_migrations()?
        .into_iter()
        .filter_map(|up_path| {
            let name = up_path
                .file_stem()
                .and_then(|stem| stem.to_str())?
                .to_string();
            Some(LocalMigration {
                down_path: manager.get_down_migration_path(&name),
                applied: applied.contains(&name),
                journal_entry: journal
                    .entries
                    .iter()
                    .find(|entry| entry.migration == name)
                    .cloned(),
                name,
                up_path,
            })
        })
        .collect::<Vec<_>>();
    migrations.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(migrations)
}

fn select_migration<'a>(
    migrations: &'a [LocalMigration],
    migration: &Option<String>,
) -> Result<Option<&'a LocalMigration>> {
    match migration {
        Some(migration) => {
            let found = migrations
                .iter()
                .rev()
                .find(|entry| entry.name == *migration || entry.name.ends_with(migration));
            if found.is_none() {
                println!(
                    "\n{}: Unable to find migration {}",
                    "Warning".yellow(),
                    migration.cyan()
                );
            }
            Ok(found)
        }
        None => prompt_for_migration(migrations),
    }
}

fn prompt_for_migration(migrations: &[LocalMigration]) -> Result<Option<&LocalMigration>> {
    let names = migrations
        .iter()
        .map(|entry| {
            if entry.applied {
                format!("{} (applied)", entry.name)
            } else {
                entry.name.clone()
            }
        })
        .collect::<Vec<_>>();

    let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select migration to drop (esc to cancel)")
        .default(0)
        .items(&names)
        .highlight_matches(true)
        .interact_opt()
        .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?;

    if let Some(idx) = selection {
        Ok(Some(&migrations[idx]))
    } else {
        println!("\n{}", "Canceled".dimmed());
        Ok(None)
    }
}

fn confirm_drop(name: &str) -> Result<bool> {
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Are you sure you want to remove migration {}?",
            name.red()
        ))
        .default(false)
        .interact()
        .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))
}

fn drop_migration(manager: &MigrationManager, migration: &LocalMigration) -> Result<()> {
    if migration.up_path.exists() {
        std::fs::remove_file(&migration.up_path)?;
    }

    let has_down = migration.down_path.exists();
    if has_down {
        std::fs::remove_file(&migration.down_path)?;
    }

    let mut removed_snapshots = 0usize;
    if let Some(snapshot_path) = migration
        .journal_entry
        .as_ref()
        .and_then(|entry| entry.snapshot_path.as_deref())
    {
        let path = manager.out_dir.join(snapshot_path);
        if path.exists() {
            std::fs::remove_file(path)?;
            removed_snapshots += 1;
        }
    }
    removed_snapshots += manager.remove_snapshots_for_migration(&migration.name)?;

    let mut journal = manager.load_journal()?;
    journal
        .entries
        .retain(|entry| entry.migration != migration.name);
    manager.save_journal(&journal)?;

    println!("{} {}", "Dropped:".green(), migration.name);
    if has_down {
        println!(
            "{} {}",
            "Dropped down:".green(),
            migration.down_path.display()
        );
    }
    if removed_snapshots > 0 {
        println!(
            "{} {}",
            "Dropped snapshot(s):".green(),
            removed_snapshots.to_string().cyan()
        );
    }

    Ok(())
}
