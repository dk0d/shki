use super::introspect::introspect_db;
use crate::config::Config;
use crate::create_any_pool_opts;
use crate::queries;
use crate::{
    MigrationManager, Result, ShkiError, Snapshot, SqlGenerator, diff_snapshots, sql_checksum,
};
use chrono::Utc;
use colored::Colorize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub async fn cmd_squash(
    config: &Config,
    name: Option<String>,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let manager = build_migration_manager(config);

    let existing_migrations = manager.list_migrations()?;
    if existing_migrations.is_empty() {
        return Err(ShkiError::config(
            "No existing migrations found. Use bootstrap for first-time adoption.",
        ));
    }

    let latest_snapshot = manager
        .load_latest_snapshot()?
        .ok_or_else(|| ShkiError::config("No snapshots found in migrations/_meta"))?;

    let desired_snapshot = Snapshot::from_config(config)?;
    let pending_lua_diff = diff_snapshots(&latest_snapshot, &desired_snapshot)?;
    if !pending_lua_diff.is_empty() {
        return Err(ShkiError::config(
            "Lua schema has pending changes. Run `shki generate` first and apply all migrations before squashing.",
        ));
    }

    let pool = create_any_pool_opts()
        .max_connections(2)
        .connect(db_url)
        .await?;

    manager.validate_snapshots()?;
    manager.validate_checksums(&pool).await?;

    let applied = manager.get_applied_migrations(&pool).await?;
    let local_names = collect_migration_names(&existing_migrations)?;
    let applied_names: BTreeSet<String> = applied.iter().map(|m| m.name.clone()).collect();
    if local_names != applied_names && !force {
        let local_only: Vec<String> = local_names.difference(&applied_names).cloned().collect();
        let db_only: Vec<String> = applied_names.difference(&local_names).cloned().collect();
        return Err(ShkiError::config(format!(
            "Applied migrations do not match local files (local-only: {}; db-only: {}). Apply/fix drift before squashing or use --force if intentional.",
            format_name_list(&local_only),
            format_name_list(&db_only),
        )));
    }

    println!("{}", "Introspecting database for squash baseline...".cyan());
    let mut baseline_snapshot = introspect_db(config).await?;
    baseline_snapshot
        .tables
        .shift_remove(&config.migrations.table);

    let sql = SqlGenerator::new(config.dialect)
        .with_breakpoints(config.breakpoints)
        .generate_sql(&diff_snapshots(
            &Snapshot::new(config.dialect),
            &baseline_snapshot,
        )?)?;

    let archive_dir = archive_dir_path(config.out_dir().as_path());
    let archive_target = next_archive_target(&archive_dir);

    if dry_run {
        println!("\n{}", "Squash plan (dry run):".cyan());
        println!("  - Existing migrations: {}", existing_migrations.len());
        println!("  - Archive dir: {}", archive_target.display());
        println!(
            "  - New migration name: {}",
            name.as_deref().unwrap_or("squash")
        );
        println!("  - Migration table rows to reset: {}", applied.len());
        return Ok(());
    }

    move_existing_to_archive(config.out_dir().as_path(), &archive_target)?;

    let migration_name = Some(name.unwrap_or_else(|| "squash".to_string()));
    let (up_path, _down_path) =
        manager.create_migration_with_down(migration_name, &sql, None, None, &baseline_snapshot)?;

    reset_migrations_table_to(&manager, &pool, &up_path, config).await?;

    println!("\n{}", "Squash complete".green());
    println!("  New migration: {}", up_path.display());
    println!("  Archived previous state: {}", archive_target.display());

    Ok(())
}

fn build_migration_manager(config: &Config) -> MigrationManager {
    let manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    if let Some(schema) = &config.migrations.schema {
        manager.with_table_schema(schema)
    } else {
        manager
    }
}

fn archive_dir_path(out_dir: &Path) -> PathBuf {
    out_dir.join("_archive")
}

fn next_archive_target(archive_dir: &Path) -> PathBuf {
    let base = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let initial = archive_dir.join(&base);
    if !initial.exists() {
        return initial;
    }

    for idx in 1.. {
        let candidate = archive_dir.join(format!("{}-{:02}", base, idx));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("archive suffix search should always find an unused path")
}

fn collect_migration_names(paths: &[PathBuf]) -> Result<BTreeSet<String>> {
    paths
        .iter()
        .map(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    ShkiError::migration(format!("Invalid migration filename: {}", path.display()))
                })
        })
        .collect()
}

fn format_name_list(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_string();
    }
    names.join(", ")
}

fn move_existing_to_archive(out_dir: &Path, archive_target: &Path) -> Result<()> {
    fs::create_dir_all(archive_target)?;

    for entry in fs::read_dir(out_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let file_name = name.to_string_lossy();

        if file_name == "_archive" {
            continue;
        }

        if is_migration_file(&path) || file_name == "_meta" {
            let target = archive_target.join(file_name.as_ref());
            fs::rename(&path, target)?;
        }
    }

    Ok(())
}

fn is_migration_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.ends_with(".sql")
}

async fn reset_migrations_table_to(
    manager: &MigrationManager,
    pool: &sqlx::AnyPool,
    migration_path: &Path,
    config: &Config,
) -> Result<()> {
    manager.ensure_migrations_table(pool).await?;

    let clear_query = queries::clear_migrations(
        &config.dialect,
        config.migrations.schema.as_deref(),
        &config.migrations.table,
    );
    let insert_query = queries::insert_migration(
        &config.dialect,
        config.migrations.schema.as_deref(),
        &config.migrations.table,
    );

    let sql = std::fs::read_to_string(migration_path)?;
    let checksum = sql_checksum(&sql);
    let name = migration_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;

    let mut tx = pool.begin().await?;
    sqlx::query(&clear_query).execute(&mut *tx).await?;
    sqlx::query(&insert_query)
        .bind(name)
        .bind(&checksum)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_migration_file() {
        assert!(is_migration_file(Path::new("001_init.sql")));
        assert!(is_migration_file(Path::new("001_init.down.sql")));
        assert!(!is_migration_file(Path::new("README.md")));
    }

    #[test]
    fn test_move_existing_to_archive_moves_sql_and_meta() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let out_dir = temp.path().join("migrations");
        fs::create_dir_all(out_dir.join("_meta")).expect("failed to create _meta");
        fs::write(out_dir.join("0000_init.sql"), "SELECT 1;").expect("failed to write migration");
        fs::write(out_dir.join("0000_init.down.sql"), "SELECT 1;")
            .expect("failed to write down migration");
        fs::write(out_dir.join("notes.txt"), "ignore").expect("failed to write notes file");

        let archive_target = out_dir.join("_archive/20260101120000");
        move_existing_to_archive(&out_dir, &archive_target).expect("failed to archive files");

        assert!(!out_dir.join("0000_init.sql").exists());
        assert!(!out_dir.join("0000_init.down.sql").exists());
        assert!(!out_dir.join("_meta").exists());
        assert!(out_dir.join("notes.txt").exists());

        assert!(archive_target.join("0000_init.sql").exists());
        assert!(archive_target.join("0000_init.down.sql").exists());
        assert!(archive_target.join("_meta").exists());
    }

    #[test]
    fn test_move_existing_to_archive_skips_archive_root() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let out_dir = temp.path().join("migrations");
        fs::create_dir_all(out_dir.join("_archive/old")).expect("failed to create _archive/old");
        fs::write(out_dir.join("0001_next.sql"), "SELECT 1;").expect("failed to write migration");

        let archive_target = out_dir.join("_archive/20260101120000");
        move_existing_to_archive(&out_dir, &archive_target).expect("failed to archive files");

        assert!(out_dir.join("_archive/old").exists());
        assert!(archive_target.join("0001_next.sql").exists());
    }

    #[test]
    fn test_next_archive_target_avoids_collisions() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let archive_dir = temp.path().join("_archive");
        fs::create_dir_all(&archive_dir).expect("failed to create archive dir");

        let first = next_archive_target(&archive_dir);
        fs::create_dir_all(&first).expect("failed to create first target");

        let second = next_archive_target(&archive_dir);
        assert_ne!(first, second);
        assert!(
            second
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.contains('-'))
        );
    }

    #[test]
    fn test_collect_migration_names_from_paths() {
        let names = collect_migration_names(&[
            PathBuf::from("0000_init.sql"),
            PathBuf::from("0001_add_users.sql"),
        ])
        .expect("failed to collect migration names");

        assert!(names.contains("0000_init"));
        assert!(names.contains("0001_add_users"));
    }
}
