pub mod rename;
pub mod statements;
pub use statements::*;
mod helpers;
mod table;
pub(crate) mod topological;

use crate::compiler::compiler_from_config;
use crate::config::Config;
use crate::migrate::manager::MigrationManager;
use crate::{Result, ShkiError};

use self::rename::{RenameDecision, RenameKind, RenameMap, RenameScenario};
use crate::models::iden::Iden;
use crate::schema::{Constraint, ForeignKeyConstraint, IndexColumn};

use super::schema::Table;
use super::snapshots::Snapshot;

pub async fn cmd_diff(config: &Config) -> Result<()> {
    let baseline = crate::compiler::resolve_baseline_snapshot(config).await?;
    let desired = compiler_from_config(config)?.compile(config).await?;
    let diff = diff_snapshots(&baseline, &desired)?;
    let preview = diff_preview(config, &diff)?;

    println!("{}", preview);

    Ok(())
}

/// The most recent committed Snapshot — the latest Journal entry that has a
/// `<migration>.snapshot.json` on disk, regardless of migration kind.
///
/// Custom migrations are snapshotted lazily (see
/// [`crate::compiler::resolve_baseline_snapshot`]); once backfilled they are
/// valid baselines too, so this no longer filters on `MigrationKind::Schema`.
pub fn load_latest_snapshot(config: &Config) -> Result<Snapshot> {
    let manager = MigrationManager::new(
        config.out_dir(),
        crate::engines::Engine::detached(config.dialect(), config.migrations.entity()),
    );
    let journal = manager.load_journal()?;
    let meta_dir = manager.meta_dir();
    let Some(entry) = journal.entries.iter().rev().find(|entry| {
        meta_dir
            .join(format!("{}.snapshot.json", entry.migration))
            .exists()
    }) else {
        return Ok(Snapshot::new(config.dialect()));
    };

    load_snapshot_by_name(config, &entry.migration)
}

/// Load the committed Snapshot recorded for a specific migration name.
pub fn load_snapshot_by_name(config: &Config, migration: &str) -> Result<Snapshot> {
    let snapshot_path = config
        .out_dir()
        .join("_meta")
        .join(format!("{}.snapshot.json", migration));
    let content = std::fs::read_to_string(&snapshot_path).map_err(|err| {
        ShkiError::schema(format!(
            "Failed to read baseline Snapshot {}: {}",
            snapshot_path.display(),
            err
        ))
    })?;
    Ok(serde_json::from_str(&content)?)
}

pub fn diff_preview(_config: &Config, diff: &SchemaDiff) -> Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("Statements: {}", diff.len()));
    lines.push(format!(
        "Rename candidates: {}",
        diff.rename_scenarios.len()
    ));
    lines.push(format!(
        "Possible Destructive changes: {}",
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

    append_change_summary(&mut lines, diff);
    append_rename_candidates(&mut lines, &diff.rename_scenarios);

    Ok(lines.join("\n"))
}

fn append_change_summary(lines: &mut Vec<String>, diff: &SchemaDiff) {
    let summary = diff.summary();
    lines.push("Changes:".to_string());

    let mut wrote = false;
    wrote |= append_summary_group(lines, "Schemas created", &summary.schemas_created);
    wrote |= append_summary_group(lines, "Schemas dropped", &summary.schemas_dropped);
    wrote |= append_summary_group(lines, "Schemas renamed", &summary.schemas_renamed);
    wrote |= append_summary_group(lines, "Extensions created", &summary.extensions_created);
    wrote |= append_summary_group(lines, "Extensions dropped", &summary.extensions_dropped);
    wrote |= append_summary_group(lines, "Enums created", &summary.enums_created);
    wrote |= append_summary_group(lines, "Enums dropped", &summary.enums_dropped);
    wrote |= append_summary_group(lines, "Enums renamed", &summary.enums_renamed);
    wrote |= append_summary_group(lines, "Enums altered", &summary.enums_altered);
    wrote |= append_summary_group(lines, "Enum values added", &summary.enum_values_added);
    wrote |= append_summary_group(lines, "Sequences created", &summary.sequences_created);
    wrote |= append_summary_group(lines, "Sequences dropped", &summary.sequences_dropped);
    wrote |= append_summary_group(lines, "Sequences altered", &summary.sequences_altered);
    wrote |= append_summary_group(lines, "Tables created", &summary.tables_created);
    wrote |= append_summary_group(lines, "Tables dropped", &summary.tables_dropped);
    wrote |= append_summary_group(lines, "Tables renamed", &summary.tables_renamed);
    wrote |= append_summary_group(lines, "Tables altered", &summary.tables_altered);
    wrote |= append_summary_group(lines, "Columns added", &summary.columns_added);
    wrote |= append_summary_group(lines, "Columns dropped", &summary.columns_dropped);
    wrote |= append_summary_group(lines, "Columns renamed", &summary.columns_renamed);
    wrote |= append_summary_group(lines, "Columns altered", &summary.columns_altered);
    wrote |= append_summary_group(lines, "Indexes created", &summary.indexes_created);
    wrote |= append_summary_group(lines, "Indexes dropped", &summary.indexes_dropped);
    wrote |= append_summary_group(lines, "Indexes renamed", &summary.indexes_renamed);
    wrote |= append_summary_group(lines, "Constraints added", &summary.constraints_added);
    wrote |= append_summary_group(lines, "Constraints dropped", &summary.constraints_dropped);
    wrote |= append_summary_group(lines, "Constraints renamed", &summary.constraints_renamed);
    wrote |= append_summary_group(lines, "Views created", &summary.views_created);
    wrote |= append_summary_group(lines, "Views dropped", &summary.views_dropped);
    wrote |= append_summary_group(lines, "Views altered", &summary.views_altered);

    if !wrote {
        lines.push("  (no categorized changes)".to_string());
    }
}

fn append_summary_group(lines: &mut Vec<String>, label: &str, values: &[String]) -> bool {
    if values.is_empty() {
        return false;
    }

    lines.push(format!("  {}: {}", label, values.len()));
    for value in values {
        lines.push(format!("    - {}", value));
    }
    true
}

fn append_rename_candidates(lines: &mut Vec<String>, scenarios: &[RenameScenario]) {
    lines.push(String::new());
    lines.push("Rename Candidates:".to_string());

    if scenarios.is_empty() {
        lines.push("  (none)".to_string());
        return;
    }

    for scenario in scenarios {
        let scope = scenario
            .table
            .as_ref()
            .map(|table| format!(" on table {}", table.name))
            .unwrap_or_default();
        lines.push(format!("  {}{}:", scenario.kind, scope));

        for dropped in scenario.dropped.values() {
            for created in scenario.created.values() {
                lines.push(format!("    - {} -> {}", dropped.name, created.name));
            }
        }
    }
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

    // Diff composite types
    helpers::diff_composite_types(
        &from.composite_types(),
        &to.composite_types(),
        &mut statements,
    );

    // Diff sequences
    helpers::diff_sequences(&from.sequences(), &to.sequences(), &mut statements);

    // Diff tables
    helpers::diff_tables(&from.tables(), &to.tables(), &from.dialect, &mut statements);

    // Diff views
    helpers::diff_views(&from.views(), &to.views(), &mut statements);

    let mut rename_scenarios = helpers::detect_type_renames(
        &from.enums(),
        &to.enums(),
        &from.composite_types(),
        &to.composite_types(),
    );
    rename_scenarios.extend(helpers::detect_table_renames(&from_tables, &to_tables));

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

/// Resolve rename decisions by applying them to the baseline Snapshot and
/// re-diffing against the desired Snapshot. Renamed objects become in-place
/// modifications, so the single diff implementation handles everything else
/// that changed alongside the rename (column types, index definitions, ...).
/// The rename statements themselves are prepended in dependency order.
pub fn apply_rename_decisions(
    from: &Snapshot,
    to: &Snapshot,
    decisions: &[RenameDecision],
) -> Result<SchemaDiff> {
    let mut renames: Vec<&RenameMap> = decisions
        .iter()
        .filter_map(|decision| match decision {
            RenameDecision::Rename(rename) => Some(rename),
            RenameDecision::Drop(_) => None,
        })
        .collect();
    // Parents before children: nested renames address objects by the new table name.
    renames.sort_by_key(|rename| match rename.source.kind {
        RenameKind::Type => 0,
        RenameKind::Table => 1,
        RenameKind::Column => 2,
        RenameKind::Index => 3,
        RenameKind::Constraint => 4,
    });

    let mut renamed = from.clone();
    for rename in &renames {
        apply_rename_to_snapshot(&mut renamed, rename)?;
    }

    let mut diff = diff_snapshots(&renamed, to)?;
    let mut statements = renames
        .iter()
        .map(|rename| rename_statement(rename))
        .collect::<Result<Vec<_>>>()?;
    statements.append(&mut diff.statements);
    diff.statements = statements;
    Ok(diff)
}

fn apply_rename_to_snapshot(snapshot: &mut Snapshot, rename: &RenameMap) -> Result<()> {
    let source = &rename.source;
    let target = &rename.target;
    let missing = || {
        ShkiError::diff(format!(
            "rename source ({}) {} not found in baseline Snapshot",
            source.kind, source.name
        ))
    };

    match source.kind {
        RenameKind::Type => {
            if let Some(mut db_enum) = snapshot.remove_enum(&source.table) {
                db_enum.name = target.name.clone();
                snapshot.insert_enum(target.table.clone(), db_enum);
                return Ok(());
            }
            let mut composite = snapshot
                .remove_composite_type(&source.table)
                .ok_or_else(missing)?;
            composite.name = target.name.clone();
            snapshot.insert_composite_type(target.table.clone(), composite);
            Ok(())
        }
        RenameKind::Table => {
            let mut table = snapshot.remove_table(&source.table).ok_or_else(missing)?;
            table.name = target.name.clone();
            snapshot.insert_table(target.table.clone(), table);
            // Postgres follows table renames in foreign keys; mirror that so the
            // re-diff doesn't drop and recreate every referencing constraint.
            for_each_foreign_key(snapshot, |fk| {
                if same_object(&fk.references, &source.table) {
                    fk.references = target.table.clone();
                }
            });
            Ok(())
        }
        RenameKind::Column => {
            let table = snapshot.table_mut(&source.table).ok_or_else(missing)?;
            let mut column = table
                .columns
                .shift_remove(&source.name)
                .ok_or_else(missing)?;
            column.name = target.name.clone();
            table.columns.insert(target.name.clone(), column);
            // Postgres follows column renames in indexes and constraints; mirror
            // that so the re-diff doesn't rebuild them.
            for index in table.indexes.values_mut() {
                for index_column in &mut index.columns {
                    if let IndexColumn::Column { name, .. } = index_column
                        && name == &source.name
                    {
                        *name = target.name.clone();
                    }
                }
            }
            for constraint in &mut table.constraints {
                rename_constraint_column(constraint, &source.name, &target.name);
            }
            for_each_foreign_key(snapshot, |fk| {
                if same_object(&fk.references, &source.table) {
                    for referenced in &mut fk.references_columns {
                        if referenced == &source.name {
                            *referenced = target.name.clone();
                        }
                    }
                }
            });
            Ok(())
        }
        RenameKind::Index => {
            let table = snapshot.table_mut(&source.table).ok_or_else(missing)?;
            let mut index = table
                .indexes
                .shift_remove(&source.name)
                .ok_or_else(missing)?;
            index.name = target.name.clone();
            table.indexes.insert(target.name.clone(), index);
            Ok(())
        }
        RenameKind::Constraint => {
            let table = snapshot.table_mut(&source.table).ok_or_else(missing)?;
            table
                .constraints
                .iter_mut()
                .find(|constraint| constraint.name() == Some(source.name.as_str()))
                .ok_or_else(missing)?
                .set_name(target.name.clone());
            Ok(())
        }
    }
}

fn for_each_foreign_key(snapshot: &mut Snapshot, mut f: impl FnMut(&mut ForeignKeyConstraint)) {
    for schema in snapshot.catalog.schemas.values_mut() {
        for table in schema.tables.values_mut() {
            for constraint in &mut table.constraints {
                if let Constraint::ForeignKey(fk) = constraint {
                    f(fk);
                }
            }
        }
    }
}

fn rename_constraint_column(constraint: &mut Constraint, from: &str, to: &str) {
    let columns = match constraint {
        Constraint::PrimaryKey(c) => &mut c.columns,
        Constraint::Unique(c) => &mut c.columns,
        Constraint::ForeignKey(c) => &mut c.columns,
        // ponytail: check/exclusion embed columns in expressions; the re-diff
        // falls back to drop+recreate for those.
        Constraint::Check(_) | Constraint::Exclusion(_) => return,
    };
    for column in columns {
        if column == from {
            *column = to.to_string();
        }
    }
}

/// Iden equality with the default schema normalized (None == Some("public")).
fn same_object(a: &Iden, b: &Iden) -> bool {
    a.name == b.name
        && a.schema.as_deref().unwrap_or("public") == b.schema.as_deref().unwrap_or("public")
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
    use crate::migrate::journal::{Journal, JournalEntry, MigrationKind};
    use crate::models::iden::Iden;
    use crate::schema::DataType;
    use crate::schema::{
        Column, CompositeType, CompositeTypeColumn, Constraint, DbEnum, ForeignKeyConstraint,
        Index, PrimaryKeyConstraint, SqlDialect, Table,
    };
    use crate::snapshots::Snapshot;
    use indexmap::IndexMap;
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
    fn diff_preview_prints_change_summary_without_sql() {
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
            common: crate::CommonArgs {
                dialect: Some(SqlDialect::Postgres),
                ..Default::default()
            },
            ..Config::default()
        };

        let preview = diff_preview(&config, &diff).expect("preview should render");

        assert!(preview.contains("Statements: 1"));
        assert!(preview.contains("Destructive changes: no"));
        assert!(preview.contains("Changes:"));
        assert!(preview.contains("Tables created: 1"));
        assert!(preview.contains("- users"));
        assert!(preview.contains("Rename Candidates:"));
        assert!(preview.contains("(none)"));
        assert!(!preview.contains("CREATE TABLE"));
        assert!(!preview.contains("SQL Preview"));
    }

    #[test]
    fn diff_preview_prints_rename_candidates() {
        let mut baseline = Snapshot::new(SqlDialect::Postgres);
        baseline.id = "baseline".to_string();
        baseline.insert_table(Iden::new("accounts", None), Table::new("accounts"));
        let mut desired = Snapshot::new(SqlDialect::Postgres);
        desired.id = "desired".to_string();
        desired.insert_table(Iden::new("users", None), Table::new("users"));
        let diff = diff_snapshots(&baseline, &desired).expect("snapshot diff should succeed");
        let config = Config {
            common: crate::CommonArgs {
                dialect: Some(SqlDialect::Postgres),
                ..Default::default()
            },
            ..Config::default()
        };

        let preview = diff_preview(&config, &diff).expect("preview should render");

        assert!(preview.contains("Rename candidates: 1"));
        assert!(preview.contains("Rename Candidates:"));
        assert!(preview.contains("table:"));
        assert!(preview.contains("accounts -> users"));
    }

    #[test]
    fn load_latest_snapshot_reads_last_schema_journal_entry_with_snapshot() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let out_dir = temp_dir.path().join("migrations");
        let meta_dir = out_dir.join("_meta");
        std::fs::create_dir_all(&meta_dir).expect("failed to create meta dir");

        let mut first = Snapshot::new(SqlDialect::Postgres);
        first.id = "first".to_string();
        let first_path = meta_dir.join("0001_schema.snapshot.json");
        std::fs::write(
            &first_path,
            serde_json::to_string_pretty(&first).expect("failed to serialize snapshot"),
        )
        .expect("failed to write first snapshot");

        let mut second = Snapshot::new(SqlDialect::Postgres);
        second.id = "second".to_string();
        let second_path = meta_dir.join("0002_schema.snapshot.json");
        std::fs::write(
            &second_path,
            serde_json::to_string_pretty(&second).expect("failed to serialize snapshot"),
        )
        .expect("failed to write second snapshot");

        Journal {
            version: "1".to_string(),
            entries: vec![
                JournalEntry {
                    index: 0,
                    migration: "0000_custom".to_string(),
                    kind: MigrationKind::Custom,
                    checksum: "custom".to_string(),
                },
                JournalEntry {
                    index: 1,
                    migration: "0001_schema".to_string(),
                    kind: MigrationKind::Schema,
                    checksum: "schema-1".to_string(),
                },
                JournalEntry {
                    index: 2,
                    migration: "0002_schema".to_string(),
                    kind: MigrationKind::Schema,
                    checksum: "schema-2".to_string(),
                },
            ],
        }
        .save(&crate::migrate::journal::journal_path(&out_dir))
        .expect("failed to write journal");

        let config = Config {
            root: temp_dir.path().to_path_buf(),
            common: crate::CommonArgs {
                migrations_dir: Some(out_dir),
                dialect: Some(SqlDialect::Postgres),
                ..Default::default()
            },
            ..Config::default()
        };

        let snapshot = load_latest_snapshot(&config).expect("latest snapshot should load");

        assert_eq!(snapshot.id, "second");
    }

    #[test]
    fn load_latest_snapshot_uses_latest_snapshotted_entry_including_custom() {
        // Once a custom migration has been backfilled with a Snapshot, it is a
        // valid baseline — load_latest_snapshot must pick it over an earlier
        // schema Snapshot rather than skipping it for being `Custom`.
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let out_dir = temp_dir.path().join("migrations");
        let meta_dir = out_dir.join("_meta");
        std::fs::create_dir_all(&meta_dir).expect("failed to create meta dir");

        for (name, id) in [("0001_schema", "schema"), ("0002_custom", "custom")] {
            let mut snap = Snapshot::new(SqlDialect::Postgres);
            snap.id = id.to_string();
            std::fs::write(
                meta_dir.join(format!("{name}.snapshot.json")),
                serde_json::to_string_pretty(&snap).expect("serialize snapshot"),
            )
            .expect("write snapshot");
        }

        Journal {
            version: "1".to_string(),
            entries: vec![
                JournalEntry {
                    index: 0,
                    migration: "0001_schema".to_string(),
                    kind: MigrationKind::Schema,
                    checksum: "schema".to_string(),
                },
                JournalEntry {
                    index: 1,
                    migration: "0002_custom".to_string(),
                    kind: MigrationKind::Custom,
                    checksum: "custom".to_string(),
                },
            ],
        }
        .save(&crate::migrate::journal::journal_path(&out_dir))
        .expect("failed to write journal");

        let config = Config {
            root: temp_dir.path().to_path_buf(),
            common: crate::CommonArgs {
                migrations_dir: Some(out_dir),
                dialect: Some(SqlDialect::Postgres),
                ..Default::default()
            },
            ..Config::default()
        };

        let snapshot = load_latest_snapshot(&config).expect("latest snapshot should load");

        assert_eq!(snapshot.id, "custom");
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
    fn creates_referenced_tables_before_referencing_tables_and_defers_foreign_keys() {
        let from = Snapshot::new(SqlDialect::Postgres);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut child = Table::in_schema("child", "public");
        child.column(Column::new("id", DataType::Integer));
        child.column(Column::new("parent_id", DataType::Integer));
        child.constraint(Constraint::ForeignKey(
            ForeignKeyConstraint::new(
                vec!["parent_id"],
                Iden::new("parent", Some("public".to_string())),
                vec!["id"],
            )
            .named("child_parent_fkey"),
        ));
        to.insert_table(Iden::new("child", Some("public".to_string())), child);

        let mut parent = Table::in_schema("parent", "public");
        parent.column(Column::new("id", DataType::Integer));
        parent.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["id"]).named("parent_pkey"),
        ));
        to.insert_table(Iden::new("parent", Some("public".to_string())), parent);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert!(matches!(
            &diff.statements[0],
            DiffStatement::CreateTable { table } if table.name == "parent"
        ));
        assert!(matches!(
            &diff.statements[1],
            DiffStatement::CreateTable { table }
                if table.name == "child" && !table.constraints.iter().any(|constraint| matches!(constraint, Constraint::ForeignKey(_)))
        ));
        assert!(matches!(
            &diff.statements[2],
            DiffStatement::AddConstraint { table, constraint, .. }
                if table == "child" && matches!(constraint, Constraint::ForeignKey(_))
        ));
    }

    #[test]
    fn creates_standalone_indexes_for_new_tables() {
        let from = Snapshot::new(SqlDialect::Postgres);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut table = Table::in_schema("item", "public");
        table.column(Column::new(
            "created_at",
            DataType::Timestamp {
                with_timezone: true,
                precision: None,
            },
        ));
        table.index(Index::new("ix_item_created_at", vec!["created_at"]));
        to.insert_table(Iden::new("item", Some("public".to_string())), table);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert!(matches!(
            &diff.statements[..],
            [
                DiffStatement::CreateTable { table },
                DiffStatement::CreateIndex { table: index_table, schema, index, .. }
            ] if table.name == "item"
                && index_table == "item"
                && schema.as_deref() == Some("public")
                && index.name == "ix_item_created_at"
        ));
    }

    #[test]
    fn exposes_and_applies_enum_rename_candidate() {
        let mut from_enums = IndexMap::new();
        from_enums.insert(
            Iden::new("eventstatus", Some("public".to_string())),
            DbEnum::with_values("eventstatus", vec!["UNPUBLISHED", "PUBLISHED", "FAILED"])
                .in_schema("public"),
        );
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.set_enums(from_enums);

        let mut to_enums = IndexMap::new();
        to_enums.insert(
            Iden::new("event_status", Some("public".to_string())),
            DbEnum::with_values("event_status", vec!["UNPUBLISHED", "PUBLISHED", "FAILED"])
                .in_schema("public"),
        );
        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.set_enums(to_enums);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert!(diff.rename_scenarios.iter().any(|scenario| {
            scenario.kind == RenameKind::Type
                && scenario.dropped.contains_key("eventstatus")
                && scenario.created.contains_key("event_status")
        }));

        let diff = apply_rename_decisions(
            &from,
            &to,
            &[RenameDecision::Rename(RenameMap::type_(
                Iden::new("eventstatus", Some("public".to_string())),
                Iden::new("event_status", Some("public".to_string())),
            ))],
        )
        .expect("rename decision should apply");

        assert_eq!(diff.statements.len(), 1);
        assert!(matches!(
            &diff.statements[0],
            DiffStatement::RenameType { from, to, schema }
                if from == "eventstatus" && to == "event_status" && schema.as_deref() == Some("public")
        ));
    }

    #[test]
    fn exposes_and_applies_composite_type_rename_candidate() {
        let columns = vec![CompositeTypeColumn {
            name: "lat".to_string(),
            data_type: DataType::DoublePrecision,
        }];

        let mut from_composites = IndexMap::new();
        from_composites.insert(
            Iden::new("geo_point", Some("public".to_string())),
            CompositeType {
                name: "geo_point".to_string(),
                schema: Some("public".to_string()),
                columns: columns.clone(),
                description: None,
            },
        );
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.set_composite_types(from_composites);

        let mut to_composites = IndexMap::new();
        to_composites.insert(
            Iden::new("coordinate", Some("public".to_string())),
            CompositeType {
                name: "coordinate".to_string(),
                schema: Some("public".to_string()),
                columns,
                description: None,
            },
        );
        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.set_composite_types(to_composites);

        let diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");

        assert!(diff.rename_scenarios.iter().any(|scenario| {
            scenario.kind == RenameKind::Type
                && scenario.dropped.contains_key("geo_point")
                && scenario.created.contains_key("coordinate")
        }));

        let diff = apply_rename_decisions(
            &from,
            &to,
            &[RenameDecision::Rename(RenameMap::type_(
                Iden::new("geo_point", Some("public".to_string())),
                Iden::new("coordinate", Some("public".to_string())),
            ))],
        )
        .expect("rename decision should apply");

        assert_eq!(diff.statements.len(), 1);
        assert!(matches!(
            &diff.statements[0],
            DiffStatement::RenameType { from, to, schema }
                if from == "geo_point" && to == "coordinate" && schema.as_deref() == Some("public")
        ));
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

        let resolved = apply_rename_decisions(
            &from,
            &to,
            &[RenameDecision::Rename(RenameMap::table(
                Iden::new("accounts", None),
                Iden::new("users", None),
            ))],
        )
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

        let resolved = apply_rename_decisions(
            &from,
            &to,
            &[
                RenameDecision::Rename(RenameMap::table(
                    Iden::new("accounts", None),
                    Iden::new("users", None),
                )),
                RenameDecision::Rename(RenameMap::column(
                    Iden::new("users", None),
                    "email",
                    "primary_email",
                )),
            ],
        )
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
