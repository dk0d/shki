// Repro for: table rename leaves duplicate CreateIndex statements behind.
use shki::diff::rename::{RenameDecision, RenameMap};
use shki::diff::{DiffStatement, apply_rename_decisions};
use shki::models::iden::Iden;
use shki::schema::{Column, DataType, Index, SqlDialect, Table};
use shki::snapshots::Snapshot;

#[test]
fn table_rename_does_not_duplicate_create_index() {
    let mut from = Snapshot::new(SqlDialect::Postgres);
    let mut old = Table::in_schema("original_table", "public");
    old.column(Column::new("id", DataType::Integer));
    old.column(Column::new("foreign_id", DataType::Integer));
    old.index(Index::new(
        "ix_original_table_foreign_id",
        vec!["foreign_id"],
    ));
    from.insert_table(Iden::new("original_table", Some("public".to_string())), old);

    let mut to = Snapshot::new(SqlDialect::Postgres);
    let mut new = Table::in_schema("renamed_table", "public");
    new.column(Column::new("id", DataType::Integer));
    new.column(Column::new("foreign_id", DataType::Integer));
    new.column(Column::new("another_id", DataType::Integer));
    new.index(Index::new(
        "renamed_table_foreign_id_idx",
        vec!["foreign_id"],
    ));
    new.index(Index::new(
        "renamed_table_another_id_idx",
        vec!["another_id"],
    ));
    to.insert_table(Iden::new("renamed_table", Some("public".to_string())), new);

    let diff = apply_rename_decisions(
        &from,
        &to,
        &[RenameDecision::Rename(RenameMap::table(
            Iden::new("original_table", Some("public".to_string())),
            Iden::new("renamed_table", Some("public".to_string())),
        ))],
    )
    .expect("rename decision should apply");

    let creates: Vec<_> = diff
        .statements
        .iter()
        .filter_map(|stmt| match stmt {
            DiffStatement::CreateIndex { index, .. } => Some(index.name.clone()),
            _ => None,
        })
        .collect();

    let another_count = creates
        .iter()
        .filter(|n| *n == "renamed_table_another_id_idx")
        .count();
    assert_eq!(
        another_count, 1,
        "CreateIndex duplicated after table rename: {creates:?}"
    );
}

#[test]
fn table_rename_preserves_column_type_change() {
    let mut from = Snapshot::new(SqlDialect::Postgres);
    let mut old = Table::in_schema("original_table", "public");
    old.column(Column::new("id", DataType::Integer));
    old.column(Column::new("payload", DataType::Integer));
    from.insert_table(Iden::new("original_table", Some("public".to_string())), old);

    let mut to = Snapshot::new(SqlDialect::Postgres);
    let mut new = Table::in_schema("renamed_table", "public");
    new.column(Column::new("id", DataType::Integer));
    new.column(Column::new("payload", DataType::Text)); // type changed
    to.insert_table(Iden::new("renamed_table", Some("public".to_string())), new);

    let diff = apply_rename_decisions(
        &from,
        &to,
        &[RenameDecision::Rename(RenameMap::table(
            Iden::new("original_table", Some("public".to_string())),
            Iden::new("renamed_table", Some("public".to_string())),
        ))],
    )
    .expect("rename decision should apply");

    let touches_payload = diff.statements.iter().any(|stmt| match stmt {
        DiffStatement::AlterColumn { column, .. } => column == "payload",
        DiffStatement::AddColumn { column, .. } => column.name == "payload",
        DiffStatement::DropColumn { column, .. } => column == "payload",
        _ => false,
    });
    assert!(
        touches_payload,
        "column type change silently lost through table rename: {:?}",
        diff.statements
    );
}

#[test]
fn table_rename_follows_foreign_key_references() {
    use shki::schema::{Constraint, ForeignKeyConstraint};

    fn child_table(parent: &str) -> Table {
        let mut child = Table::in_schema("child", "public");
        child.column(Column::new("id", DataType::Integer));
        child.column(Column::new("parent_id", DataType::Integer));
        child.constraint(Constraint::ForeignKey(
            ForeignKeyConstraint::new(
                vec!["parent_id"],
                Iden::new(parent, Some("public".to_string())),
                vec!["id"],
            )
            .named("child_parent_id_fkey"),
        ));
        child
    }

    let mut parent = Table::in_schema("parent", "public");
    parent.column(Column::new("id", DataType::Integer));
    let mut from = Snapshot::new(SqlDialect::Postgres);
    from.insert_table(Iden::new("parent", Some("public".to_string())), parent);
    from.insert_table(
        Iden::new("child", Some("public".to_string())),
        child_table("parent"),
    );

    let mut guardian = Table::in_schema("guardian", "public");
    guardian.column(Column::new("id", DataType::Integer));
    let mut to = Snapshot::new(SqlDialect::Postgres);
    to.insert_table(Iden::new("guardian", Some("public".to_string())), guardian);
    to.insert_table(
        Iden::new("child", Some("public".to_string())),
        child_table("guardian"),
    );

    let diff = apply_rename_decisions(
        &from,
        &to,
        &[RenameDecision::Rename(RenameMap::table(
            Iden::new("parent", Some("public".to_string())),
            Iden::new("guardian", Some("public".to_string())),
        ))],
    )
    .expect("rename decision should apply");

    // The FK in child should follow the rename (as Postgres does), not be
    // dropped and recreated.
    assert!(
        matches!(
            &diff.statements[..],
            [DiffStatement::RenameTable { from, to, .. }]
                if from == "parent" && to == "guardian"
        ),
        "expected only RenameTable, got: {:?}",
        diff.statements
    );
}
