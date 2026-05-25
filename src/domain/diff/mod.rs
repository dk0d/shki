pub mod rename;
pub mod statements;
pub use statements::*;
mod helpers;

use crate::Result;
use crate::config::Config;

use self::rename::RenameScenario;

use super::schema::Table;
use super::snapshots::Snapshot;

pub async fn cmd_diff(config: &Config) -> Result<()> {
    Ok(())
}

pub fn diff_snapshots(from: &Snapshot, to: &Snapshot) -> Result<SchemaDiff> {
    let mut statements = Vec::new();

    // Diff extensions (PostgreSQL)
    helpers::diff_extensions(&from.extensions, &to.extensions, &mut statements);

    // Diff schemas
    helpers::diff_schemas(&from.schemas, &to.schemas, &mut statements);

    // Diff enums
    helpers::diff_enums(&from.enums, &to.enums, &mut statements);

    // Diff sequences
    helpers::diff_sequences(&from.sequences, &to.sequences, &mut statements);

    // Diff tables
    helpers::diff_tables(&from.tables, &to.tables, &from.dialect, &mut statements);

    // Diff views
    helpers::diff_views(&from.views, &to.views, &mut statements);

    let mut rename_scenarios = helpers::detect_table_renames(&from.tables, &to.tables);

    // detect column renames where the table names haven't changed,
    // need to do another pass

    for (name, from_table) in &from.tables {
        if let Some(to_table) = to.tables.get(name) {
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
    use crate::models::iden::Iden;
    use crate::schema::{Column, Constraint, Index, PrimaryKeyConstraint, SqlDialect, Table};
    use crate::snapshots::Snapshot;

    #[test]
    fn diffs_extensions_and_schemas() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.extensions = vec!["pgcrypto".to_string()];
        from.schemas = vec!["public".to_string(), "legacy".to_string()];

        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.extensions = vec!["uuid-ossp".to_string()];
        to.schemas = vec!["public".to_string(), "analytics".to_string()];

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
    fn skips_builtin_schemas() {
        let mut from = Snapshot::new(SqlDialect::Postgres);
        from.schemas = vec!["public".to_string(), "main".to_string()];

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
        from.tables.insert(Iden::new("users", None), from_table);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut to_table = Table::new("users");
        to_table.column(Column::new("primary_email", crate::schema::DataType::Text));
        to_table.index(Index::new("users_primary_email_idx", vec!["email"]));
        to_table.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["email"]).named("users_primary_email_key"),
        ));
        to.tables.insert(Iden::new("users", None), to_table);

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
        from.tables
            .insert(Iden::new("accounts", None), Table::new("accounts"));

        let mut to = Snapshot::new(SqlDialect::Postgres);
        to.tables
            .insert(Iden::new("users", None), Table::new("users"));

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
        from.tables.insert(Iden::new("accounts", None), old_table);

        let mut to = Snapshot::new(SqlDialect::Postgres);
        let mut new_table = Table::new("users");
        new_table.column(Column::new("primary_email", crate::schema::DataType::Text));
        to.tables.insert(Iden::new("users", None), new_table);

        let mut diff = diff_snapshots(&from, &to).expect("snapshot diff should succeed");
        detect_nested_renames(
            from.tables.get(&Iden::new("accounts", None)).unwrap(),
            to.tables.get(&Iden::new("users", None)).unwrap(),
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
