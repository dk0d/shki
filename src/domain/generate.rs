use std::fmt::Write as _;
use std::path::Path;

use chrono::Utc;
use colored::Colorize;

use crate::compiler::compiler_from_config;
use crate::config::Config;
use crate::create;
use crate::diff::rename::{RenameDecision, RenameKind, RenameScenario};
use crate::diff::{detect_nested_renames, diff_preview, diff_snapshots, load_latest_snapshot};
use crate::migrate::journal::MigrationKind;
use crate::migrate::manager::MigrationManager;
use crate::migrate::utils::sanitize_migration_name;
use crate::snapshots::Snapshot;
use crate::sql::render::SqlRenderer;
use crate::tui::prompt_for_rename;
use crate::{Result, ShkiError};

/// Generate a migration from the latest Snapshot and current Declarative Schema.
pub async fn cmd_generate(
    config: &Config,
    name: &str,
    custom: bool,
    with_down: bool,
) -> Result<()> {
    if custom {
        return create::cmd_create(config, name, None, None, with_down, false).await;
    }

    let manager = MigrationManager::new(
        config.out_dir(),
        crate::engines::Engine::detached(config.dialect, config.migrations.entity()),
    );
    manager.ensure_dir()?;

    let baseline = load_latest_snapshot(config)?;
    let compiler = compiler_from_config(config)?;
    let mut desired = compiler.compile(config).await?;
    desired.prev_id = Some(baseline.id.clone());

    let mut diff = diff_snapshots(&baseline, &desired)?;
    if diff.is_empty() {
        println!("{}", "No schema changes detected.".yellow());
        return Ok(());
    }
    if diff.has_rename_scenarios() {
        let mut decisions = prompt_for_rename(&diff.rename_scenarios).await?;
        let nested = nested_rename_scenarios_for_table_decisions(&baseline, &desired, &decisions);
        if !nested.is_empty() {
            let nested_decisions = prompt_for_rename(&nested).await?;
            decisions.extend(nested_decisions);
        }

        diff.rename_scenarios.extend(nested);
        diff = diff.apply_rename_decisions(&decisions)?;
    }

    let generator = SqlRenderer::new(&config.dialect);
    let up_sql = generator.generate_string(&diff.statements)?;
    compiler
        .validate_generated_diff_sql(config, &baseline, &up_sql)
        .await?;
    let down_sql = if with_down || config.migrations.generate_down {
        let (down_diff, irreversible) = diff.get_down_diff();
        let mut sql = generator.generate_string(&down_diff.statements)?;
        if !irreversible.is_empty() {
            if !sql.is_empty() && !sql.ends_with('\n') {
                sql.push('\n');
            }
            writeln!(
                &mut sql,
                "-- {} irreversible statement(s) could not be rendered as a Down Migration.",
                irreversible.len()
            )
            .expect("writing to String cannot fail");
        }
        Some(sql)
    } else {
        None
    };

    let migration_name = manager.next_migration_name(Some(&sanitize_migration_name(name)))?;
    let up_path = manager.out_dir.join(format!("{}.sql", migration_name));
    let down_path = down_sql
        .as_ref()
        .map(|_| manager.out_dir.join(format!("{}.down.sql", migration_name)));
    let snapshot_path = manager
        .meta_dir()
        .join(format!("{}.snapshot.json", migration_name));

    write_schema_migration(&up_path, &migration_name, &up_sql, false)?;
    if let (Some(path), Some(sql)) = (&down_path, down_sql.as_deref()) {
        write_schema_migration(path, &migration_name, sql, true)?;
    }
    std::fs::write(&snapshot_path, desired.to_json()?)?;
    manager.record_migration_in_journal(&up_path, MigrationKind::Schema)?;

    println!("{} {}", "Generated migration:".green(), migration_name);
    println!("\nUp:       {}", up_path.display());
    if let Some(path) = &down_path {
        println!("Down:     {}", path.display());
    }
    println!("Snapshot: {}", snapshot_path.display());
    println!();
    println!("{}", diff_preview(config, &diff)?);

    Ok(())
}

pub(crate) fn write_schema_migration(
    path: &Path,
    migration_name: &str,
    sql: &str,
    down: bool,
) -> Result<()> {
    if path.exists() {
        return Err(ShkiError::migration(format!(
            "Migration file already exists: {}",
            path.display()
        )));
    }

    let mut content = String::new();
    writeln!(
        &mut content,
        "-- Migration: {} ({})",
        migration_name,
        if down { "down" } else { "up" }
    )
    .expect("writing to String cannot fail");
    writeln!(&mut content, "-- Created at: {}", Utc::now().to_rfc3339())
        .expect("writing to String cannot fail");
    content.push_str("-- Type: schema\n\n");
    content.push_str(sql);
    if !content.ends_with('\n') {
        content.push('\n');
    }

    std::fs::write(path, content)?;
    Ok(())
}

fn nested_rename_scenarios_for_table_decisions(
    baseline: &Snapshot,
    desired: &Snapshot,
    decisions: &[RenameDecision],
) -> Vec<RenameScenario> {
    let baseline_tables = baseline.tables();
    let desired_tables = desired.tables();
    let mut nested = Vec::new();

    for decision in decisions {
        let RenameDecision::Rename(rename) = decision else {
            continue;
        };
        if rename.source.kind != RenameKind::Table {
            continue;
        }
        let Some(from) = baseline_tables.get(&rename.source.table) else {
            continue;
        };
        let Some(to) = desired_tables.get(&rename.target.table) else {
            continue;
        };
        detect_nested_renames(from, to, &mut nested, false);
    }

    nested
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::rename::RenameMap;
    use crate::models::iden::Iden;
    use crate::schema::{Column, DataType, SqlDialect, Table};
    use tempfile::TempDir;

    #[test]
    fn write_schema_migration_adds_schema_header_and_sql() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let path = temp_dir.path().join("0000_create_users.sql");

        write_schema_migration(
            &path,
            "0000_create_users",
            "CREATE TABLE users (id int);",
            false,
        )
        .expect("migration should write");

        let content = std::fs::read_to_string(&path).expect("migration should be readable");
        assert!(content.starts_with("-- Migration: 0000_create_users (up)\n"));
        assert!(content.contains("-- Type: schema\n"));
        assert!(content.contains("CREATE TABLE users (id int);\n"));
    }

    #[test]
    fn write_schema_migration_rejects_existing_file() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let path = temp_dir.path().join("0000_existing.sql");
        std::fs::write(&path, "already here").expect("failed to seed file");

        let error = write_schema_migration(&path, "0000_existing", "SELECT 1;", false)
            .expect_err("existing file should fail");

        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn nested_rename_scenarios_are_detected_for_table_rename_decisions() {
        let mut baseline = Snapshot::new(SqlDialect::Postgres);
        let mut old_table = Table::new("accounts");
        old_table.column(Column::new("email", DataType::Text));
        baseline.insert_table(Iden::new("accounts", None), old_table);

        let mut desired = Snapshot::new(SqlDialect::Postgres);
        let mut new_table = Table::new("users");
        new_table.column(Column::new("primary_email", DataType::Text));
        desired.insert_table(Iden::new("users", None), new_table);

        let nested = nested_rename_scenarios_for_table_decisions(
            &baseline,
            &desired,
            &[RenameDecision::Rename(RenameMap::table(
                Iden::new("accounts", None),
                Iden::new("users", None),
            ))],
        );

        assert!(nested.iter().any(|scenario| {
            scenario.kind == RenameKind::Column
                && scenario.dropped.contains_key("email")
                && scenario.created.contains_key("primary_email")
        }));
    }
}
