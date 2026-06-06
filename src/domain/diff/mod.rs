pub mod rename;
pub mod statements;
pub use statements::*;
mod helpers;

use std::path::PathBuf;

use crate::compiler::compiler_from_config;
use crate::config::Config;
use crate::migrate::journal::MigrationKind;
use crate::migrate::manager::MigrationManager;
use crate::sql::generator::SqlGenerator;
use crate::{Result, ShkiError};

use self::rename::RenameScenario;

use super::schema::Table;
use super::snapshots::Snapshot;

pub async fn cmd_diff(config: &Config) -> Result<()> {
    let baseline = load_latest_snapshot(config)?;
    let desired = compiler_from_config(config)?.compile(config).await?;
    let diff = diff_snapshots(&baseline, &desired)?;
    let preview = diff_preview(config, &baseline, &desired, &diff)?;

    println!("{}", preview);

    Ok(())
}

pub fn load_latest_snapshot(config: &Config) -> Result<Snapshot> {
    let manager = MigrationManager::new(
        config.out_dir(),
        crate::engines::Engine::detached(config.dialect, config.migrations.entity()),
    );
    let journal = manager.load_journal()?;
    let Some(entry) = journal
        .entries
        .iter()
        .rev()
        .find(|entry| entry.kind == MigrationKind::Schema && entry.snapshot_path.is_some())
    else {
        return Ok(Snapshot::new(config.dialect));
    };

    let snapshot_path = entry
        .snapshot_path
        .as_deref()
        .map(PathBuf::from)
        .expect("snapshot_path is present because journal entry was filtered");
    let snapshot_path = if snapshot_path.is_absolute() {
        snapshot_path
    } else {
        config.root.join(snapshot_path)
    };
    let content = std::fs::read_to_string(&snapshot_path).map_err(|err| {
        ShkiError::schema(format!(
            "Failed to read baseline Snapshot {}: {}",
            snapshot_path.display(),
            err
        ))
    })?;
    Ok(serde_json::from_str(&content)?)
}

pub fn diff_preview(
    config: &Config,
    baseline: &Snapshot,
    desired: &Snapshot,
    diff: &SchemaDiff,
) -> Result<String> {
    let mut lines = Vec::new();
    lines.push("Shki Diff Preview".to_string());
    lines.push(format!("Baseline Snapshot: {}", snapshot_label(baseline)));
    lines.push(format!("Desired Snapshot: {}", snapshot_label(desired)));
    lines.push(format!("Statements: {}", diff.len()));
    lines.push(format!(
        "Rename candidates: {}",
        diff.rename_scenarios.len()
    ));
    lines.push(format!(
        "Destructive changes: {}",
        if diff.has_destructive_changes() {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(String::new());

    if diff.is_empty() {
        lines.push("No schema changes detected.".to_string());
        return Ok(lines.join("\n"));
    }

    lines.push("SQL Preview:".to_string());
    lines.push(String::new());
    let generator = SqlGenerator::new(&config.dialect);
    lines.push(generator.generate_string(&diff.statements)?);

    Ok(lines.join("\n"))
}

fn snapshot_label(snapshot: &Snapshot) -> String {
    if snapshot.id.is_empty() {
        "<empty>".to_string()
    } else {
        snapshot.id.clone()
    }
}

pub fn diff_snapshots(from: &Snapshot, to: &Snapshot) -> Result<SchemaDiff> {
    let mut statements = Vec::new();
    let from_tables = from.tables();
    let to_tables = to.tables();

    // Diff extensions (PostgreSQL)
    helpers::diff_extensions(&from.extensions(), &to.extensions(), &mut statements);

    // Diff schemas
    helpers::diff_schemas(&from.schemas(), &to.schemas(), &mut statements);

    // Diff enums
    helpers::diff_enums(&from.enums(), &to.enums(), &mut statements);

    // Diff sequences
    helpers::diff_sequences(&from.sequences(), &to.sequences(), &mut statements);

    // Diff tables
    helpers::diff_tables(&from.tables(), &to.tables(), &from.dialect, &mut statements);

    // Diff views
    helpers::diff_views(&from.views(), &to.views(), &mut statements);

    let mut rename_scenarios = helpers::detect_table_renames(&from_tables, &to_tables);

    // detect column renames where the table names haven't changed,
    // need to do another pass

    for (name, from_table) in &from_tables {
        if let Some(to_table) = to_tables.get(name) {
            detect_nested_renames(from_table, to_table, &mut rename_scenarios, true);
        }
    }

    Ok(SchemaDiff {
        statements,
        rename_scenarios,
    })
}

pub fn detect_nested_renames(
    from: &Table,
    to: &Table,
    rename_scenarios: &mut Vec<RenameScenario>,
    require_same_table_name: bool,
) {
    // detect column renames where the table names haven't changed,
    // need to do another pass
    rename_scenarios.extend(helpers::detect_column_renames(
        from,
        to,
        require_same_table_name,
    ));
    rename_scenarios.extend(helpers::detect_index_renames(
        from,
        to,
        require_same_table_name,
    ));
    rename_scenarios.extend(helpers::detect_constraint_renames(
        from,
        to,
        require_same_table_name,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::rename::{RenameDecision, RenameKind, RenameMap};
    use crate::migrate::journal::{Journal, JournalEntry};
    use crate::models::iden::Iden;
    use crate::schema::DataType;
    use crate::schema::{Column, Constraint, Index, PrimaryKeyConstraint, SqlDialect, Table};
    use crate::snapshots::Snapshot;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn diffs_extensions_and_schemas() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.set_extensions(vec!["pgcrypto".to_string()]);
        from.set_schemas(vec!["public".to_string(), "legacy".to_string()]);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.set_extensions(vec!["uuid-ossp".to_string()]);
        to.set_schemas(vec!["public".to_string(), "analytics".to_string()]);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert_eq!(diff.len(), 4);
        assert!(matches!(
            &diff.statements[0],
            DiffStatement::CreateExtension(name) if name == "uuid-ossp"
        ));
        assert!(matches!(
            &diff.statements[1],
            DiffStatement::DropExtension(name) if name == "pgcrypto"
        ));
        assert!(matches!(
            &diff.statements[2],
            DiffStatement::CreateSchema { name } if name == "analytics"
        ));
        assert!(matches!(
            &diff.statements[3],
            DiffStatement::DropSchema { name, cascade: false } if name == "legacy"
        ));
    }

    #[test]
    fn diff_preview_prints_summary_and_sql() {
        let mut baseline = Snapshot::new(SqlDialect::Postgres);
        baseline.id = "baseline".to_string();
        let mut desired = Snapshot::new(SqlDialect::Postgres);
        desired.id = "desired".to_string();
        desired.insert_table(Iden::new("users", None), {
            let mut table = Table::new("users");
            table.column(Column::new("id", DataType::Integer));
            table
        });
        let diff = diff_snapshots(&baseline, &desired).expect("snapshot diff should succeed");
        let config = Config {
            dialect: SqlDialect::Postgres,
            ..Config::default()
        };

        let preview =
            diff_preview(&config, &baseline, &desired, &diff).expect("preview should render SQL");

        assert!(preview.contains("Shki Diff Preview"));
        assert!(preview.contains("Baseline Snapshot: baseline"));
        assert!(preview.contains("Desired Snapshot: desired"));
        assert!(preview.contains("Statements: 1"));
        assert!(preview.contains("Destructive changes: no"));
        assert!(preview.contains("CREATE TABLE"));
    }

    #[test]
    fn load_latest_snapshot_reads_last_schema_journal_entry_with_snapshot() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let out_dir = temp_dir.path().join("migrations");
        let meta_dir = out_dir.join("_meta");
        std::fs::create_dir_all(&meta_dir).expect("failed to create meta dir");

        let mut first = Snapshot::new(SqlDialect::Postgres);
        first.id = "first".to_string();
        let first_path = meta_dir.join("first_snapshot.json");
        std::fs::write(
            &first_path,
            serde_json::to_string_pretty(&first).expect("failed to serialize snapshot"),
        )
        .expect("failed to write first snapshot");

        let mut second = Snapshot::new(SqlDialect::Postgres);
        second.id = "second".to_string();
        let second_path = meta_dir.join("second_snapshot.json");
        std::fs::write(
            &second_path,
            serde_json::to_string_pretty(&second).expect("failed to serialize snapshot"),
        )
        .expect("failed to write second snapshot");

        Journal {
            version: "1".to_string(),
            entries: vec![
                JournalEntry {
                    migration: "0000_custom".to_string(),
                    kind: MigrationKind::Custom,
                    checksum: "custom".to_string(),
                    snapshot_path: None,
                    snapshot_id: None,
                    prev_snapshot_id: None,
                    created_at: Utc::now(),
                },
                JournalEntry {
                    migration: "0001_schema".to_string(),
                    kind: MigrationKind::Schema,
                    checksum: "schema-1".to_string(),
                    snapshot_path: Some(first_path.to_string_lossy().to_string()),
                    snapshot_id: Some("first".to_string()),
                    prev_snapshot_id: None,
                    created_at: Utc::now(),
                },
                JournalEntry {
                    migration: "0002_schema".to_string(),
                    kind: MigrationKind::Schema,
                    checksum: "schema-2".to_string(),
                    snapshot_path: Some(second_path.to_string_lossy().to_string()),
                    snapshot_id: Some("second".to_string()),
                    prev_snapshot_id: Some("first".to_string()),
                    created_at: Utc::now(),
                },
            ],
        }
        .save(&crate::migrate::journal::journal_path(&out_dir))
        .expect("failed to write journal");

        let config = Config {
            root: temp_dir.path().to_path_buf(),
            out: out_dir,
            dialect: SqlDialect::Postgres,
            ..Config::default()
        };

        let snapshot = load_latest_snapshot(&config).expect("latest snapshot should load");

        assert_eq!(snapshot.id, "second");
    }

    #[test]
    fn skips_builtin_schemas() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.set_schemas(vec!["public".to_string(), "main".to_string()]);

        let to = Snapshot::new(SqlDialect::Postgres);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert!(diff.is_empty());
    }

    #[test]
    fn exposes_rename_candidates_for_same_table_objects() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        let mut from_table = Table::new("users");
        from_table.column(Column::new("email", crate::schema::DataType::Text));
        from_table.index(Index::new("users_email_idx", vec!["email"]));
        from_table.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["email"]).named("users_email_key"),
        ));
        from.insert_table(Iden::new("users", None), from_table);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut to_table = Table::new("users");
        to_table.column(Column::new("primary_email", crate::schema::DataType::Text));
        to_table.index(Index::new("users_primary_email_idx", vec!["email"]));
        to_table.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["email"]).named("users_primary_email_key"),
        ));
        to.insert_table(Iden::new("users", None), to_table);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert_eq!(diff.rename_scenarios.len(), 3);
        assert!(diff.rename_scenarios.iter().any(|scenario| {
            scenario.kind == RenameKind::Column
                && scenario.dropped.contains_key("email")
                && scenario.created.contains_key("primary_email")
        }));
        assert!(diff.rename_scenarios.iter().any(|scenario| {
            scenario.kind == RenameKind::Index
                && scenario.dropped.contains_key("users_email_idx")
                && scenario.created.contains_key("users_primary_email_idx")
        }));
        assert!(diff.rename_scenarios.iter().any(|scenario| {
            scenario.kind == RenameKind::Constraint
                && scenario.dropped.contains_key("users_email_key")
                && scenario.created.contains_key("users_primary_email_key")
        }));
    }

    #[test]
    fn applies_table_rename_decision_with_nested_renames() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.insert_table(Iden::new("accounts", None), Table::new("accounts"));

        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.insert_table(Iden::new("users", None), Table::new("users"));

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");
        assert_eq!(diff.rename_scenarios.len(), 1);
        assert_eq!(diff.statements.len(), 2);

        let resolved = diff
            .apply_rename_decisions(&[RenameDecision::Rename(RenameMap::table(
                Iden::new("accounts", None),
                Iden::new("users", None),
            ))])
            .expect("rename decision should apply");

        assert!(matches!(
            &resolved.statements[..],
            [DiffStatement::RenameTable { from, to, .. }]
                if from == "accounts" && to == "users"
        ));
    }

    #[test]
    fn applies_table_and_nested_column_rename_decisions() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        let mut old_table = Table::new("accounts");
        old_table.column(Column::new("email", crate::schema::DataType::Text));
        from.insert_table(Iden::new("accounts", None), old_table);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut new_table = Table::new("users");
        new_table.column(Column::new("primary_email", crate::schema::DataType::Text));
        to.insert_table(Iden::new("users", None), new_table);

        let mut diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");
        let from_tables = from.tables();
        let to_tables = to.tables();
        detect_nested_renames(
            from_tables.get(&Iden::new("accounts", None)).unwrap(),
            to_tables.get(&Iden::new("users", None)).unwrap(),
            &mut diff.rename_scenarios,
            false,
        );

        let resolved = diff
            .apply_rename_decisions(&[
                RenameDecision::Rename(RenameMap::table(
                    Iden::new("accounts", None),
                    Iden::new("users", None),
                )),
                RenameDecision::Rename(RenameMap::column(
                    Iden::new("users", None),
                    "email",
                    "primary_email",
                )),
            ])
            .expect("rename decisions should apply");

        assert!(matches!(
            &resolved.statements[..],
            [
                DiffStatement::RenameTable { from, to, .. },
                DiffStatement::RenameColumn { table, from: column_from, to: column_to, .. }
            ] if from == "accounts"
                && to == "users"
                && table == "users"
                && column_from == "email"
                && column_to == "primary_email"
        ));
    }
}
