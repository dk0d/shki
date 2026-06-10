use indexmap::IndexMap;

use crate::models::iden::Iden;
use crate::schema::{Column, Constraint, Index, SqlDialect, Table};

use super::topological::sort_created_tables;
use super::{DiffStatement, helpers};

#[derive(Debug, Clone)]
pub(super) enum TableDiff {
    Create(Table),
    Drop(Table),
    Modify { from: Table, to: Table },
}

pub(super) fn diff_tables(
    from: &IndexMap<Iden, Table>,
    to: &IndexMap<Iden, Table>,
    dialect: &SqlDialect,
    statements: &mut Vec<DiffStatement>,
) {
    let mut created = Vec::new();
    let mut dropped = Vec::new();
    let mut modified = Vec::new();

    for (id, table_to) in to {
        if !from.contains_key(id) {
            created.push(TableDiff::Create(table_to.clone()));
        }
    }

    for (id, table_from) in from {
        if !to.contains_key(id) {
            dropped.push(TableDiff::Drop(table_from.clone()));
        }
    }

    for (id, table_to) in to {
        if let Some(table_from) = from.get(id) {
            modified.push(TableDiff::Modify {
                from: table_from.clone(),
                to: table_to.clone(),
            });
        }
    }

    lower_table_diffs(created, dropped, modified, dialect, statements);
}

fn lower_table_diffs(
    created: Vec<TableDiff>,
    dropped: Vec<TableDiff>,
    modified: Vec<TableDiff>,
    dialect: &SqlDialect,
    statements: &mut Vec<DiffStatement>,
) {
    let created_tables = created
        .into_iter()
        .filter_map(|diff| match diff {
            TableDiff::Create(table) => Some(table),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut deferred_foreign_keys = Vec::new();
    for table in sort_created_tables(created_tables) {
        let (table_without_fks, foreign_keys) = split_foreign_keys(table);
        statements.push(DiffStatement::CreateTable {
            table: table_without_fks.clone(),
        });
        for constraint in foreign_keys {
            deferred_foreign_keys.push(DiffStatement::AddConstraint {
                table: table_without_fks.name.clone(),
                schema: table_without_fks.schema.clone(),
                constraint,
            });
        }
    }

    for diff in dropped {
        if let TableDiff::Drop(table) = diff {
            statements.push(DiffStatement::DropTable {
                name: table.name.clone(),
                schema: table.schema.clone(),
                cascade: false,
                prev: table,
            });
        }
    }

    for diff in modified {
        if let TableDiff::Modify { from, to } = diff {
            helpers::diff_table(&from, &to, dialect, statements);
        }
    }

    statements.extend(deferred_foreign_keys);
}

fn split_foreign_keys(mut table: Table) -> (Table, Vec<Constraint>) {
    let mut inline_constraints = Vec::new();
    let mut foreign_keys = Vec::new();

    for constraint in table.constraints {
        if matches!(constraint, Constraint::ForeignKey(_)) {
            foreign_keys.push(constraint);
        } else {
            inline_constraints.push(constraint);
        }
    }

    table.constraints = inline_constraints;
    (table, foreign_keys)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(super) struct DetailedTableDiff {
    pub table: Table,
    pub added_columns: Vec<Column>,
    pub dropped_columns: Vec<Column>,
    pub modified_columns: Vec<super::ColumnChange>,
    pub added_constraints: Vec<Constraint>,
    pub dropped_constraints: Vec<Constraint>,
    pub added_indexes: Vec<Index>,
    pub dropped_indexes: Vec<Index>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, DataType, ForeignKeyConstraint, PrimaryKeyConstraint};

    #[test]
    fn created_table_foreign_keys_are_deferred() {
        let mut table = Table::in_schema("child", "public");
        table.column(Column::new("id", DataType::Integer));
        table.column(Column::new("parent_id", DataType::Integer));
        table.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["id"]).named("child_pkey"),
        ));
        table.constraint(Constraint::ForeignKey(
            ForeignKeyConstraint::new(
                vec!["parent_id"],
                Iden::new("parent", Some("public".to_string())),
                vec!["id"],
            )
            .named("child_parent_fkey"),
        ));

        let (without_fks, fks) = split_foreign_keys(table);

        assert_eq!(without_fks.constraints.len(), 1);
        assert_eq!(fks.len(), 1);
        assert!(matches!(fks[0], Constraint::ForeignKey(_)));
    }
}
