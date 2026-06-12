use std::collections::HashSet;

use crate::models::iden::Iden;

use super::render::{SqlObjectType, SqlStmt};

pub fn order_statements(statements: Vec<SqlStmt>) -> Vec<SqlStmt> {
    let mut non_tables = Vec::new();
    let mut tables = Vec::new();

    for statement in statements {
        if statement.object_type() == SqlObjectType::Table {
            tables.push(statement);
        } else {
            non_tables.push(statement);
        }
    }

    non_tables
        .sort_by_key(|statement| (object_order(statement.object_type()), statement.ordinal()));

    let tables = order_table_statements(tables);
    let mut ordered = Vec::new();
    for object_type in object_order_sequence() {
        if object_type == SqlObjectType::Table {
            ordered.extend(tables.iter().cloned());
        } else {
            ordered.extend(
                non_tables
                    .iter()
                    .filter(|statement| statement.object_type() == object_type)
                    .cloned(),
            );
        }
    }

    ordered
}

fn order_table_statements(statements: Vec<SqlStmt>) -> Vec<SqlStmt> {
    let table_ids = statements
        .iter()
        .filter_map(|statement| statement.identity().cloned())
        .collect::<HashSet<_>>();
    let mut remaining = statements;
    let mut emitted = HashSet::<Iden>::new();
    let mut ordered = Vec::new();

    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .enumerate()
            .filter(|(_, statement)| {
                table_dependencies_are_satisfied(statement, &table_ids, &emitted)
            })
            .min_by_key(|(_, statement)| statement.ordinal())
            .map(|(idx, _)| idx);

        let Some(next) = next else {
            remaining.sort_by_key(SqlStmt::ordinal);
            ordered.extend(remaining);
            break;
        };

        let statement = remaining.remove(next);
        if let Some(identity) = statement.identity().cloned() {
            emitted.insert(identity);
        }
        ordered.push(statement);
    }

    ordered
}

fn table_dependencies_are_satisfied(
    statement: &SqlStmt,
    table_ids: &HashSet<Iden>,
    emitted: &HashSet<Iden>,
) -> bool {
    statement
        .depends_on()
        .iter()
        .filter(|dependency| table_ids.contains(*dependency))
        .all(|dependency| emitted.contains(dependency))
}

fn object_order(object_type: SqlObjectType) -> usize {
    match object_type {
        SqlObjectType::Schema => 0,
        SqlObjectType::DefaultPrivilege => 1,
        SqlObjectType::Extension => 2,
        SqlObjectType::Type => 3,
        SqlObjectType::Function => 4,
        SqlObjectType::Procedure => 5,
        SqlObjectType::Aggregate => 6,
        SqlObjectType::Sequence => 7,
        SqlObjectType::Table => 8,
        SqlObjectType::View => 9,
        SqlObjectType::MaterializedView => 10,
        SqlObjectType::Index => 11,
        SqlObjectType::Trigger => 12,
        SqlObjectType::Policy => 13,
        SqlObjectType::Column => 14,
        SqlObjectType::Rls => 15,
        SqlObjectType::Privilege => 16,
        SqlObjectType::ColumnPrivilege => 17,
        SqlObjectType::RevokedDefaultPrivilege => 18,
        SqlObjectType::Other => 19,
    }
}

fn object_order_sequence() -> [SqlObjectType; 20] {
    [
        SqlObjectType::Schema,
        SqlObjectType::DefaultPrivilege,
        SqlObjectType::Extension,
        SqlObjectType::Type,
        SqlObjectType::Function,
        SqlObjectType::Procedure,
        SqlObjectType::Aggregate,
        SqlObjectType::Sequence,
        SqlObjectType::Table,
        SqlObjectType::View,
        SqlObjectType::MaterializedView,
        SqlObjectType::Index,
        SqlObjectType::Trigger,
        SqlObjectType::Policy,
        SqlObjectType::Column,
        SqlObjectType::Rls,
        SqlObjectType::Privilege,
        SqlObjectType::ColumnPrivilege,
        SqlObjectType::RevokedDefaultPrivilege,
        SqlObjectType::Other,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::render::SqlOperation;

    #[test]
    fn orders_objects_by_dependency_bucket() {
        let statements = vec![
            SqlStmt::from("CREATE INDEX users_id_idx ON users (id)").with_planning(
                SqlObjectType::Index,
                SqlOperation::Create,
                0,
            ),
            SqlStmt::from("CREATE TABLE users (id int)").with_planning(
                SqlObjectType::Table,
                SqlOperation::Create,
                1,
            ),
            SqlStmt::from("CREATE TYPE status AS ENUM ('active')").with_planning(
                SqlObjectType::Type,
                SqlOperation::Create,
                2,
            ),
            SqlStmt::from("CREATE SCHEMA app").with_planning(
                SqlObjectType::Schema,
                SqlOperation::Create,
                3,
            ),
        ];

        let ordered = order_statements(statements);

        assert!(ordered[0].as_sql().starts_with("CREATE SCHEMA"));
        assert!(ordered[1].as_sql().starts_with("CREATE TYPE"));
        assert!(ordered[2].as_sql().starts_with("CREATE TABLE"));
        assert!(ordered[3].as_sql().starts_with("CREATE INDEX"));
    }

    #[test]
    fn orders_tables_by_dependencies() {
        let parent = Iden::new("parent", Some("app".to_string()));
        let child = Iden::new("child", Some("app".to_string()));
        let statements = vec![
            SqlStmt::from("CREATE TABLE app.child (parent_id int)")
                .with_planning(SqlObjectType::Table, SqlOperation::Create, 0)
                .with_identity(child)
                .with_dependencies(vec![parent.clone()]),
            SqlStmt::from("CREATE TABLE app.parent (id int)")
                .with_planning(SqlObjectType::Table, SqlOperation::Create, 1)
                .with_identity(parent),
        ];

        let ordered = order_statements(statements);

        assert!(ordered[0].as_sql().starts_with("CREATE TABLE app.parent"));
        assert!(ordered[1].as_sql().starts_with("CREATE TABLE app.child"));
    }
}
