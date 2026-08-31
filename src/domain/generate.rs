use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::Path;

use chrono::Utc;
use owo_colors::OwoColorize;

use crate::compiler::compiler_from_config;
use crate::config::Config;
use crate::create;
use crate::diff::rename::{RenameDecision, RenameKind, RenameScenario};
use crate::diff::{
    DiffStatement, SchemaDiff, apply_rename_decisions, detect_nested_renames, diff_preview,
    diff_snapshots,
};
use crate::migrate::journal::MigrationKind;
use crate::migrate::manager::MigrationManager;
use crate::migrate::utils::sanitize_migration_name;
use crate::snapshots::Snapshot;
use crate::sql::render::SqlRenderer;
use crate::tui::{confirm, prompt_for_rename};
use crate::{MIGRATION_SPLIT_MARKER, Result, ShkiError};

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
        crate::engines::Engine::detached(config.dialect(), config.migrations.entity()),
    );
    manager.ensure_dir()?;

    let baseline = crate::compiler::resolve_baseline_snapshot(config).await?;
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

        diff = apply_rename_decisions(&baseline, &desired, &decisions)?;
    }

    let (main_diff, concurrent_diff) = split_concurrent_index_diff(&diff);
    if !concurrent_diff.statements.is_empty() {
        confirm_concurrent_index_split(concurrent_diff.statements.len()).await?;
    }

    let generator = SqlRenderer::new(&config.dialect());
    let main_sql = generator.generate_string(&main_diff.statements)?;

    // Validate both migrations in one shadow pass. CONCURRENTLY is stripped
    // here because validation runs inside one implicit transaction; the
    // resulting schema shape is identical.
    let validation_sql = if concurrent_diff.statements.is_empty() {
        main_sql.clone()
    } else {
        let stripped: Vec<DiffStatement> = concurrent_diff
            .statements
            .iter()
            .cloned()
            .map(strip_concurrently)
            .collect();
        let mut sql = main_sql.clone();
        if !sql.is_empty() && !sql.ends_with('\n') {
            sql.push('\n');
        }
        sql.push_str(&generator.generate_string(&stripped)?);
        sql
    };
    compiler
        .validate_generated_diff_sql(config, &baseline, &validation_sql)
        .await?;

    let with_down = with_down || config.migrations.generate_down();
    let suffix = sanitize_migration_name(name);
    let mut planned = Vec::new();
    if !main_diff.statements.is_empty() {
        planned.push(PlannedMigration {
            suffix: suffix.clone(),
            up_sql: main_sql,
            down_sql: with_down
                .then(|| render_down_sql(&generator, &main_diff))
                .transpose()?,
        });
    }
    if !concurrent_diff.statements.is_empty() {
        // Concurrent index builds go into their own no-transaction migration:
        // CREATE INDEX CONCURRENTLY refuses to run inside a transaction block.
        let index_suffix = if planned.is_empty() {
            suffix
        } else {
            format!("{suffix}-indexes")
        };
        planned.push(PlannedMigration {
            suffix: index_suffix,
            up_sql: render_no_transaction_sql(&generator, &concurrent_diff.statements)?,
            down_sql: with_down
                .then(|| {
                    let (down_diff, _) = concurrent_diff.get_down_diff();
                    render_no_transaction_sql(&generator, &down_diff.statements)
                })
                .transpose()?,
        });
    }

    write_planned_migrations(&manager, &planned, &desired)?;
    println!();
    println!("{}", diff_preview(config, &diff)?);

    Ok(())
}

fn write_planned_migrations(
    manager: &MigrationManager,
    planned: &[PlannedMigration],
    desired: &Snapshot,
) -> Result<()> {
    let mut last_migration_name = String::new();
    for migration in planned {
        let migration_name = manager.next_migration_name(Some(&migration.suffix))?;
        let up_path = manager.out_dir.join(format!("{}.sql", migration_name));
        write_schema_migration(&up_path, &migration_name, &migration.up_sql, false)?;
        println!("{} {}", "Generated migration:".green(), migration_name);
        println!("\nUp:       {}", up_path.display());
        if let Some(down_sql) = &migration.down_sql {
            let down_path = manager.out_dir.join(format!("{}.down.sql", migration_name));
            write_schema_migration(&down_path, &migration_name, down_sql, true)?;
            println!("Down:     {}", down_path.display());
        }
        manager.record_migration_in_journal(&up_path, MigrationKind::Schema)?;
        last_migration_name = migration_name;
    }

    // The Snapshot describes the state after ALL generated migrations, so it
    // belongs to the last one; earlier files stay un-snapshotted like custom
    // migrations do.
    let snapshot_path = manager
        .meta_dir()
        .join(format!("{}.snapshot.json", last_migration_name));
    std::fs::write(&snapshot_path, desired.to_json()?)?;
    println!("Snapshot: {}", snapshot_path.display());
    Ok(())
}

struct PlannedMigration {
    suffix: String,
    up_sql: String,
    down_sql: Option<String>,
}

/// Partition a diff into ordinary statements and `CREATE INDEX CONCURRENTLY`
/// statements, which must run outside a transaction in their own migration.
fn split_concurrent_index_diff(diff: &SchemaDiff) -> (SchemaDiff, SchemaDiff) {
    let (concurrent, main): (Vec<_>, Vec<_>) = diff.statements.iter().cloned().partition(
        |stmt| matches!(stmt, DiffStatement::CreateIndex { index, .. } if index.concurrently),
    );
    (
        SchemaDiff {
            statements: main,
            rename_scenarios: Vec::new(),
        },
        SchemaDiff {
            statements: concurrent,
            rename_scenarios: Vec::new(),
        },
    )
}

fn strip_concurrently(stmt: DiffStatement) -> DiffStatement {
    match stmt {
        DiffStatement::CreateIndex {
            table,
            schema,
            mut index,
        } => {
            index.concurrently = false;
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
            }
        }
        other => other,
    }
}

/// Render statements as a no-transaction migration: directive header, one
/// statement per split-marker segment so each runs as its own query.
fn render_no_transaction_sql(
    generator: &SqlRenderer,
    statements: &[DiffStatement],
) -> Result<String> {
    let rendered = statements
        .iter()
        .map(|stmt| generator.generate_string(&vec![stmt.clone()]))
        .collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "-- shki:no-transaction\n\n{}",
        rendered.join(&format!("\n{MIGRATION_SPLIT_MARKER}\n"))
    ))
}

fn render_down_sql(generator: &SqlRenderer, diff: &SchemaDiff) -> Result<String> {
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
    Ok(sql)
}

/// Indexes declared `CONCURRENTLY` change what `generate` writes (a second,
/// no-transaction migration), so the user must confirm — declining fails the
/// whole generation and nothing is written.
async fn confirm_concurrent_index_split(count: usize) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(ShkiError::migration(
            "The Declarative Schema declares CREATE INDEX CONCURRENTLY, which splits the \
             generated migration in two (schema changes, then a no-transaction migration \
             building the indexes concurrently). This requires interactive confirmation — \
             re-run `shki generate` in a terminal, or remove CONCURRENTLY from the schema.",
        ));
    }
    let prompt = format!(
        "{count} index(es) are declared CONCURRENTLY and will be written as a separate \
         no-transaction migration (CREATE INDEX CONCURRENTLY IF NOT EXISTS). Continue?"
    );
    if confirm(prompt).await? {
        Ok(())
    } else {
        Err(ShkiError::migration(
            "Migration generation cancelled: the concurrent index migration was not confirmed.",
        ))
    }
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
    use crate::schema::{Column, DataType, Index, SqlDialect, Table};
    use tempfile::TempDir;

    fn concurrent_index_diff() -> SchemaDiff {
        let mut table = Table::new("users");
        table.column(Column::new("id", DataType::Integer));
        SchemaDiff {
            statements: vec![
                DiffStatement::CreateTable { table },
                DiffStatement::CreateIndex {
                    table: "users".to_string(),
                    schema: None,
                    index: Index::new("users_email_idx", vec!["email"]).concurrently(),
                },
                DiffStatement::CreateIndex {
                    table: "users".to_string(),
                    schema: None,
                    index: Index::new("users_id_idx", vec!["id"]),
                },
            ],
            rename_scenarios: Vec::new(),
        }
    }

    #[test]
    fn splits_concurrent_index_statements_into_their_own_diff() {
        let (main, concurrent) = split_concurrent_index_diff(&concurrent_index_diff());

        assert_eq!(main.statements.len(), 2);
        assert_eq!(concurrent.statements.len(), 1);
        assert!(matches!(
            &concurrent.statements[0],
            DiffStatement::CreateIndex { index, .. } if index.name == "users_email_idx"
        ));
    }

    #[test]
    fn renders_no_transaction_migration_with_directive_and_segments() {
        let (_, concurrent) = split_concurrent_index_diff(&concurrent_index_diff());
        let mut statements = concurrent.statements;
        statements.push(DiffStatement::CreateIndex {
            table: "users".to_string(),
            schema: None,
            index: Index::new("users_name_idx", vec!["name"]).concurrently(),
        });

        let generator = SqlRenderer::new(&SqlDialect::Postgres);
        let sql = render_no_transaction_sql(&generator, &statements).expect("sql should render");

        assert!(sql.starts_with("-- shki:no-transaction\n"));
        assert!(sql.contains("CREATE INDEX CONCURRENTLY IF NOT EXISTS \"users_email_idx\""));
        assert_eq!(sql.matches(MIGRATION_SPLIT_MARKER).count(), 1);

        let down = SchemaDiff {
            statements,
            rename_scenarios: Vec::new(),
        }
        .get_down_diff()
        .0;
        let down_sql =
            render_no_transaction_sql(&generator, &down.statements).expect("down should render");
        assert!(down_sql.contains("DROP INDEX CONCURRENTLY IF EXISTS \"users_email_idx\""));
    }

    #[test]
    fn write_planned_migrations_snapshots_only_the_last_migration() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let manager = MigrationManager::new(
            temp_dir.path(),
            crate::engines::Engine::detached(
                SqlDialect::Postgres,
                Iden::new("__shki_migrations", None),
            ),
        );
        manager.ensure_dir().expect("dirs should create");

        let planned = vec![
            PlannedMigration {
                suffix: "add-users".to_string(),
                up_sql: "CREATE TABLE users (id int);".to_string(),
                down_sql: Some("DROP TABLE users;".to_string()),
            },
            PlannedMigration {
                suffix: "add-users-indexes".to_string(),
                up_sql: "-- shki:no-transaction\n\nCREATE INDEX CONCURRENTLY IF NOT EXISTS i ON users (id);".to_string(),
                down_sql: Some("-- shki:no-transaction\n\nDROP INDEX CONCURRENTLY IF EXISTS i;".to_string()),
            },
        ];
        write_planned_migrations(&manager, &planned, &Snapshot::new(SqlDialect::Postgres))
            .expect("migrations should write");

        for file in [
            "0000_add-users.sql",
            "0000_add-users.down.sql",
            "0001_add-users-indexes.sql",
            "0001_add-users-indexes.down.sql",
            "_meta/0001_add-users-indexes.snapshot.json",
        ] {
            assert!(temp_dir.path().join(file).exists(), "{file} should exist");
        }
        assert!(
            !temp_dir
                .path()
                .join("_meta/0000_add-users.snapshot.json")
                .exists(),
            "only the last migration gets the snapshot"
        );

        let journal = manager.load_journal().expect("journal should load");
        assert_eq!(
            journal
                .entries
                .iter()
                .map(|entry| entry.migration.as_str())
                .collect::<Vec<_>>(),
            ["0000_add-users", "0001_add-users-indexes"]
        );
    }

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
