use colored::Colorize;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect};

use crate::{Config, MigrationManager, Result, ShkiError};

/// Drop a migration file
pub async fn cmd_drop(config: &Config, migration: &Option<String>) -> Result<()> {
    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect);
    let migrations = migration_manager.list_migrations()?;

    if migrations.is_empty() {
        println!("\n{}", "No migrations found".yellow());
        return Ok(());
    }

    let to_drop = match migration {
        Some(migration) => {
            // search from the latest back
            let found = migrations.iter().rev().find(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s == migration || s.ends_with(migration))
                    .unwrap_or(false)
            });
            if found.is_none() {
                println!(
                    "\n{}: Unable to find migration {}",
                    "Warning".yellow(),
                    migration.cyan()
                )
            }
            found
        }
        None => {
            let names = migrations
                .iter()
                .map(|p| format!("{:?}", p.file_stem().unwrap()))
                .collect::<Vec<String>>();

            let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select migration to drop (esc to cancel)")
                .default(0)
                .items(&names)
                .highlight_matches(true)
                .interact_opt()
                .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?;

            if let Some(idx) = selection {
                // println!("Selected:  {:?}", &migrations[idx].to_str());
                Some(&migrations[idx])
            } else {
                println!("\n{}", "Canceled".dimmed());
                None
            }
        }
    };

    match to_drop {
        Some(path) => {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "Are you sure you want to remove migration {}?",
                    name.red()
                ))
                .default(false)
                .interact()
                .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?;

            if confirmed {
                let down_path = migration_manager.get_down_migration_path(path);
                let has_down = down_path.exists();

                std::fs::remove_file(path)?;
                if has_down {
                    std::fs::remove_file(&down_path)?;
                }
                let removed_snapshots = migration_manager.remove_snapshots_for_migration(name)?;

                println!("{} {}", "Dropped:".green(), name);
                if has_down {
                    println!("{} {}", "Dropped down:".green(), down_path.display());
                }
                if removed_snapshots > 0 {
                    println!(
                        "{} {}",
                        "Dropped snapshot(s):".green(),
                        removed_snapshots.to_string().cyan()
                    );
                }
            } else {
                println!("{}", "Aborted".yellow());
            }
        }
        None => {
            // println!("\n{}", "Canceled".dimmed());
        }
    }

    Ok(())
}
