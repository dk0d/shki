use colored::Colorize;

use crate::config::Config;
use crate::diff::{diff_preview, diff_snapshots, load_latest_snapshot};
use crate::dump::{render_directory_schema_preview, write_directory_schema};
use crate::engines::Engine;
use crate::generate::write_schema_migration;
use crate::migrate::checksum::sql_checksum;
use crate::migrate::journal::MigrationKind;
use crate::migrate::manager::MigrationManager;
use crate::migrate::utils::sanitize_migration_name;
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::render::SqlRenderer;
use crate::{Result, ShkiError};

use super::diff::SchemaDiff;

fn print_applied(name: &str) {
    println!(
        "{} Applied: {} {}",
        "✔".green(),
        name,
        "marked as already applied".dimmed()
    );
}

pub async fn cmd_bootstrap(
    config: &Config,
    name: Option<&str>,
    mark_applied: bool,
    mark_only: bool,
    dry_run: bool,
    force: bool,
    schema: &Option<String>,
) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

    println!("{}", "Bootstrapping from database shape...\n".cyan());

    let engine = Engine::from_config(config).await?;
    let snapshot = engine.introspect(config, schema).await?;

    let manager = MigrationManager::from_config(config).await?;

    let plan = plan_bootstrap(config, &manager, &snapshot, mark_only, name, force)?;

    if dry_run {
        println!("Initial migration: {}", plan.migration_name);
        println!("Schema path:       {}", config.schema_path().display());
        println!("Migration path:    {}", plan.up_path.display());
        println!("Snapshot path:     {}", plan.snapshot_path.display());
        println!("Mark applied:      {}", mark_applied);
        println!();
        println!("{}", render_directory_schema_preview(config, &snapshot)?);
        return Ok(());
    }

    if mark_only {
        let latest_snapshot = load_latest_snapshot(config)?;
        let diff = diff_snapshots(&latest_snapshot, &snapshot)?;
        if !diff.is_empty() {
            let preview = diff_preview(config, &diff)?;
            println!("{}", &preview);
            return Err(ShkiError::migration(
                "Schema changes found, cannot mark applied",
            ));
        }
        manager.mark_migration_applied(&plan.up_path).await?;
        print_applied(&latest_snapshot.migration.expect("must have info").name);
        return Ok(());
    }

    if plan.diff.is_empty() {
        return Err(ShkiError::migration(
            "bootstrap found no schema objects to write into an initial migration",
        ));
    }

    let result = write_bootstrap_artifacts(config, &manager, plan, snapshot, force)?;

    if mark_applied {
        manager.mark_migration_applied(&result.up_path).await?;
    }

    println!(
        "{} {}",
        "Schema written to:".green(),
        result.schema_path.display()
    );
    println!(
        "{} {}",
        "Generated migration:".green(),
        result.migration_name
    );
    println!("\nUp:       {}", result.up_path.display());
    println!("Snapshot: {}", result.snapshot_path.display());
    if mark_applied {
        print_applied(&result.migration_name);
    }

    Ok(())
}

#[derive(Debug)]
struct BootstrapPlan {
    migration_name: String,
    up_path: std::path::PathBuf,
    snapshot_path: std::path::PathBuf,
    diff: SchemaDiff,
}

#[derive(Debug)]
struct BootstrapResult {
    migration_name: String,
    schema_path: std::path::PathBuf,
    up_path: std::path::PathBuf,
    snapshot_path: std::path::PathBuf,
}

fn plan_bootstrap(
    config: &Config,
    manager: &MigrationManager,
    snapshot: &Snapshot,
    mark_only: bool,
    name: Option<&str>,
    force: bool,
) -> Result<BootstrapPlan> {
    if !force && !manager.list_up_migrations()?.is_empty() && !mark_only {
        return Err(ShkiError::migration(
            "bootstrap requires an empty migrations directory; use --force to append a bootstrap migration",
        ));
    }
    let empty = Snapshot::new(config.dialect);
    let diff = diff_snapshots(&empty, snapshot)?;

    let migration_name = if mark_only {
        latest_schema_migration_name(manager)?
    } else {
        let suffix = sanitize_migration_name(name.unwrap_or("bootstrap"));
        manager.next_migration_name(Some(&suffix))?
    };
    let up_path = manager.out_dir.join(format!("{}.sql", migration_name));
    let snapshot_path = manager
        .meta_dir()
        .join(format!("{}.snapshot.json", migration_name));

    Ok(BootstrapPlan {
        migration_name,
        up_path,
        snapshot_path,
        diff,
    })
}

fn latest_schema_migration_name(manager: &MigrationManager) -> Result<String> {
    manager
        .load_journal()?
        .entries
        .iter()
        .rev()
        .find(|entry| entry.kind == MigrationKind::Schema)
        .map(|entry| entry.migration.clone())
        .ok_or_else(|| {
            ShkiError::migration(
                "bootstrap --mark-only requires an existing schema migration in the journal",
            )
        })
}

fn write_bootstrap_artifacts(
    config: &Config,
    manager: &MigrationManager,
    plan: BootstrapPlan,
    mut snapshot: Snapshot,
    force: bool,
) -> Result<BootstrapResult> {
    let schema_path = config.schema_path();

    manager.ensure_dir()?;
    write_directory_schema(&snapshot, &schema_path, force)?;

    let up_sql = SqlRenderer::new(&config.dialect).generate_string(&plan.diff.statements)?;
    write_schema_migration(&plan.up_path, &plan.migration_name, &up_sql, false)?;

    let file_sql = std::fs::read_to_string(&plan.up_path)?;
    snapshot.migration = Some(crate::migrate::manager::MigrationInfo {
        name: plan.migration_name.clone(),
        checksum: Some(sql_checksum(&file_sql)),
    });
    std::fs::write(&plan.snapshot_path, snapshot.to_json()?)?;
    manager.record_migration_in_journal(&plan.up_path, MigrationKind::Schema)?;

    Ok(BootstrapResult {
        migration_name: plan.migration_name,
        schema_path,
        up_path: plan.up_path,
        snapshot_path: plan.snapshot_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MigrationPrefix};
    use crate::engines::Engine;
    use crate::models::iden::Iden;
    use crate::schema::{Column, DataType, SqlDialect, Table};
    use tempfile::TempDir;

    fn test_config(root: &std::path::Path) -> Config {
        let mut config = Config::default()
            .with_dialect(SqlDialect::Postgres)
            .with_root(root.to_path_buf());
        config.migrations.prefix = MigrationPrefix::Index;
        config
    }

    fn test_manager(config: &Config) -> MigrationManager {
        MigrationManager::new(
            config.out_dir(),
            Engine::detached(config.dialect, config.migrations.entity()),
        )
    }

    fn test_snapshot() -> Snapshot {
        let mut snapshot = Snapshot::new(SqlDialect::Postgres);
        let mut table = Table::in_schema("users", "public");
        table.column(Column::new("id", DataType::Integer));
        snapshot.insert_table(Iden::new("users", Some("public".to_string())), table);
        snapshot
    }

    #[test]
    fn writes_schema_migration_snapshot_and_journal() {
        let temp = TempDir::new().expect("temp dir");
        let config = test_config(temp.path());
        let manager = test_manager(&config);
        let snapshot = test_snapshot();
        let force = false;
        let name = "initial";
        let plan = plan_bootstrap(&config, &manager, &snapshot, false, Some(name), force)
            .expect("planned");
        let result = write_bootstrap_artifacts(&config, &manager, plan, snapshot, force)
            .expect("bootstrap artifacts should write");

        assert_eq!(result.migration_name, "0000_initial");
        assert!(config.schema_path().join("main.sql").exists());
        assert!(result.up_path.exists());
        assert!(result.snapshot_path.exists());

        let migration = std::fs::read_to_string(&result.up_path).expect("read migration");
        assert!(migration.contains("-- Type: schema"));
        assert!(migration.contains("CREATE TABLE \"public\".\"users\""));

        let journal = manager.load_journal().expect("load journal");
        assert_eq!(journal.entries.len(), 1);
        assert_eq!(journal.entries[0].migration, "0000_initial");
        assert_eq!(journal.entries[0].kind, MigrationKind::Schema);
    }

    #[test]
    fn refuses_existing_migrations_without_force() {
        let temp = TempDir::new().expect("temp dir");
        let config = test_config(temp.path());
        let manager = test_manager(&config);
        manager.ensure_dir().expect("ensure dir");
        std::fs::write(config.out_dir().join("0000_existing.sql"), "SELECT 1;")
            .expect("write migration");

        let snapshot = test_snapshot();
        let force = false;
        let name = "inital";
        let error = plan_bootstrap(&config, &manager, &snapshot, false, Some(name), force)
            .expect_err("planned error");
        assert!(error.to_string().contains("empty migrations directory"));
    }

    #[test]
    fn mark_only_uses_latest_schema_migration_from_journal() {
        let temp = TempDir::new().expect("temp dir");
        let config = test_config(temp.path());
        let manager = test_manager(&config);
        let snapshot = test_snapshot();
        let plan = plan_bootstrap(&config, &manager, &snapshot, false, Some("initial"), false)
            .expect("planned");
        let result = write_bootstrap_artifacts(&config, &manager, plan, snapshot.clone(), false)
            .expect("bootstrap artifacts should write");

        let mark_only_plan = plan_bootstrap(&config, &manager, &snapshot, true, None, false)
            .expect("mark-only plan should use existing migration");

        assert_eq!(mark_only_plan.migration_name, result.migration_name);
        assert_eq!(mark_only_plan.up_path, result.up_path);
    }
}
