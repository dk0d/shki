//! Adopt an existing database at a committed baseline migration.
//!
//! Authoring the baseline (see [`crate::bootstrap`]) produces committed artifacts.
//! Adoption is the per-environment act of telling a *live* database "you are already
//! at the baseline" so that newer migrations layer on top:
//!
//! 1. Introspect the live database and strictly validate its shape against the
//!    committed baseline Snapshot (refuses on drift unless `--force`).
//! 2. Mark every migration up to and including the baseline as applied, without
//!    executing their SQL.
//! 3. Apply any newer pending migrations (unless `--mark-only`).
//!
//! Fresh/empty environments don't need this — `migrate` runs the baseline like any
//! other migration.

use std::collections::HashSet;

use colored::Colorize;

use crate::config::Config;
use crate::diff::{diff_preview, diff_snapshots, load_snapshot_by_name};
use crate::display::tables::display_migrations;
use crate::engines::Engine;
use crate::migrate::journal::MigrationKind;
use crate::migrate::manager::{ApplyMigrationMode, MigrationManager};
use crate::snapshots::Introspectable;
use crate::{Result, ShkiError};

pub async fn cmd_adopt(
    config: &Config,
    name: Option<&str>,
    mark_only: bool,
    force: bool,
    dry_run: bool,
    schema: &Option<String>,
) -> Result<()> {
    config.require_database_url()?;
    config.display_sanitized_db_url();

    let manager = MigrationManager::from_config(config).await?;
    let journal = manager.load_journal()?;

    // Resolve the baseline: the requested migration, else the earliest schema migration.
    let baseline_index = resolve_baseline_index(&journal, name)?;
    let baseline_name = journal.entries[baseline_index].migration.clone();

    println!(
        "{} {}\n",
        "Adopting database at baseline:".cyan(),
        baseline_name.bold()
    );

    // Validate the live shape against the committed baseline Snapshot.
    let baseline_snapshot = load_snapshot_by_name(config, &baseline_name)?;
    let engine = Engine::from_config(config).await?;
    let live_snapshot = engine.introspect(config, schema).await?;
    let diff = diff_snapshots(&baseline_snapshot, &live_snapshot)?;

    if !diff.is_empty() {
        println!("{}", diff_preview(config, &diff)?);
        if !force {
            return Err(ShkiError::migration(format!(
                "live database differs from the '{}' baseline; reconcile the schema or re-run with --force",
                baseline_name
            )));
        }
        println!("{}", "\nProceeding despite drift (--force).".yellow());
    } else {
        println!("{}", "Live database matches the baseline.".green());
    }

    // Migrations to mark applied: everything up to and including the baseline that the
    // database hasn't already recorded.
    let already_applied: HashSet<String> = manager
        .try_get_applied_migrations()
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect();

    let to_mark: Vec<String> = journal
        .entries
        .iter()
        .filter(|entry| entry.index <= baseline_index)
        .map(|entry| entry.migration.clone())
        .filter(|migration| !already_applied.contains(migration))
        .collect();

    if dry_run {
        report_dry_run(&manager, &to_mark, &already_applied, mark_only).await?;
        return Ok(());
    }

    for migration in &to_mark {
        let path = manager.get_up_migration_path(migration);
        manager.mark_migration_applied(&path).await?;
        println!(
            "{} {} {}",
            "✔".green(),
            migration,
            "marked as applied".dimmed()
        );
    }
    if to_mark.is_empty() {
        println!("{}", "Baseline already recorded as applied.".dimmed());
    }

    if !mark_only {
        let applied = manager.apply(ApplyMigrationMode::All).await?;
        println!(
            "\n{} newer migration(s) applied",
            applied.len().to_string().green()
        );
    }

    println!();
    display_migrations(&manager, config).await?;

    Ok(())
}

/// Resolve the journal index of the baseline migration to adopt up to.
fn resolve_baseline_index(
    journal: &crate::migrate::journal::Journal,
    name: Option<&str>,
) -> Result<usize> {
    match name {
        Some(name) => journal
            .entries
            .iter()
            .position(|entry| entry.migration == name)
            .ok_or_else(|| {
                ShkiError::migration(format!("migration '{}' is not in the journal", name))
            }),
        None => journal
            .entries
            .iter()
            .position(|entry| entry.kind == MigrationKind::Schema)
            .ok_or_else(|| {
                ShkiError::migration(
                    "no schema migration found in the journal; author one with `shki bootstrap` first",
                )
            }),
    }
}

async fn report_dry_run(
    manager: &MigrationManager,
    to_mark: &[String],
    already_applied: &HashSet<String>,
    mark_only: bool,
) -> Result<()> {
    println!("\n{}", "Dry run — no changes will be made.".cyan());

    if to_mark.is_empty() {
        println!(
            "Would mark applied: {}",
            "(nothing — already recorded)".dimmed()
        );
    } else {
        println!("Would mark applied:");
        for migration in to_mark {
            println!("  - {}", migration);
        }
    }

    if mark_only {
        println!(
            "Would apply newer migrations: {}",
            "(skipped, --mark-only)".dimmed()
        );
        return Ok(());
    }

    let mark_set: HashSet<&str> = to_mark.iter().map(String::as_str).collect();
    let would_apply: Vec<String> = manager
        .list_up_migrations()?
        .into_iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .filter(|name| !already_applied.contains(name) && !mark_set.contains(name.as_str()))
        .collect();

    if would_apply.is_empty() {
        println!(
            "Would apply newer migrations: {}",
            "(none pending)".dimmed()
        );
    } else {
        println!("Would apply newer migrations:");
        for migration in would_apply {
            println!("  - {}", migration);
        }
    }

    Ok(())
}
