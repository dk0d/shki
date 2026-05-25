use std::collections::HashSet;
use std::hash::Hash;

use indexmap::IndexMap;

use crate::models::iden::Iden;
use crate::schema::{Column, Constraint, DbEnum, Index, Sequence, SqlDialect, Table, View};

use super::rename::{RenameId, RenameKind, RenameScenario};
use super::{ColumnChange, DiffStatement, EnumValuePosition, SequenceChange};

pub(super) fn diff_extensions(from: &[String], to: &[String], statements: &mut Vec<DiffStatement>) {
    diff_string_entries(
        from,
        to,
        |_| true,
        |statements, ext| statements.push(DiffStatement::CreateExtension(ext.clone())),
        |statements, ext| statements.push(DiffStatement::DropExtension(ext.clone())),
        statements,
    );
}

pub(super) fn diff_schemas(from: &[String], to: &[String], statements: &mut Vec<DiffStatement>) {
    diff_string_entries(
        from,
        to,
        |schema| schema != "public" && schema != "main",
        |statements, schema| {
            statements.push(DiffStatement::CreateSchema {
                name: schema.clone(),
            })
        },
        |statements, schema| {
            statements.push(DiffStatement::DropSchema {
                name: schema.clone(),
                cascade: false,
            })
        },
        statements,
    );
}

pub(super) fn diff_enums(
    from: &IndexMap<Iden, DbEnum>,
    to: &IndexMap<Iden, DbEnum>,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, enum_to| {
            statements.push(DiffStatement::CreateEnum {
                name: enum_to.name.clone(),
                schema: enum_to.schema.clone(),
                values: enum_to.values.clone(),
                description: enum_to.description.clone(),
            });
        },
        |statements, _, enum_from| {
            statements.push(DiffStatement::DropEnum {
                name: enum_from.name.clone(),
                schema: enum_from.schema.clone(),
                prev: enum_from.clone(),
            });
        },
        |statements, _, enum_from, enum_to| {
            // Find new values
            let existing_values: HashSet<&str> =
                enum_from.values.iter().map(String::as_str).collect();
            let mut prev_value: Option<&str> = None;
            for (idx, value) in enum_to.values.iter().enumerate() {
                if !existing_values.contains(value.as_str()) {
                    let position = match prev_value {
                        Some(pv) => EnumValuePosition::After(pv.to_owned()),
                        None => enum_to
                            .values
                            .iter()
                            .skip(idx + 1)
                            .find(|next| existing_values.contains(next.as_str()))
                            .map(|next| EnumValuePosition::Before(next.clone()))
                            .unwrap_or(EnumValuePosition::End),
                    };
                    statements.push(DiffStatement::AddEnumValue {
                        enum_name: enum_to.name.clone(),
                        schema: enum_to.schema.clone(),
                        value: value.clone(),
                        position,
                    });
                }
                prev_value = Some(value.as_str());
            }

            // Check for description changes
            if enum_from.description != enum_to.description {
                statements.push(DiffStatement::AlterEnumDescription {
                    name: enum_to.name.clone(),
                    schema: enum_to.schema.clone(),
                    description: enum_to.description.clone(),
                    prev_description: enum_from.description.clone(),
                });
            }
        },
        statements,
    );
}

pub(super) fn diff_sequences(
    from: &IndexMap<Iden, Sequence>,
    to: &IndexMap<Iden, Sequence>,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, seq_to| {
            statements.push(DiffStatement::CreateSequence {
                sequence: seq_to.clone(),
            });
        },
        |statements, _, seq_from| {
            statements.push(DiffStatement::DropSequence {
                name: seq_from.name.clone(),
                schema: seq_from.schema.clone(),
                prev: seq_from.clone(),
            });
        },
        |statements, _, seq_from, seq_to| {
            let mut changes = Vec::new();

            if seq_from.increment != seq_to.increment {
                changes.push(SequenceChange::Increment(seq_to.increment));
            }
            if seq_from.min_value != seq_to.min_value {
                changes.push(SequenceChange::MinValue(seq_to.min_value));
            }
            if seq_from.max_value != seq_to.max_value {
                changes.push(SequenceChange::MaxValue(seq_to.max_value));
            }
            if seq_from.start != seq_to.start {
                changes.push(SequenceChange::Start(seq_to.start));
            }
            if seq_from.cache != seq_to.cache {
                changes.push(SequenceChange::Cache(seq_to.cache));
            }
            if seq_from.cycle != seq_to.cycle {
                changes.push(SequenceChange::Cycle(seq_to.cycle));
            }

            if !changes.is_empty() {
                statements.push(DiffStatement::AlterSequence {
                    name: seq_to.name.clone(),
                    schema: seq_to.schema.clone(),
                    changes,
                });
            }
        },
        statements,
    );
}

pub(super) fn diff_tables(
    from: &IndexMap<Iden, Table>,
    to: &IndexMap<Iden, Table>,
    dialect: &SqlDialect,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, table_to| {
            statements.push(DiffStatement::CreateTable {
                table: table_to.clone(),
            });
        },
        |statements, _, table_from| {
            statements.push(DiffStatement::DropTable {
                name: table_from.name.clone(),
                schema: table_from.schema.clone(),
                cascade: false,
                prev: table_from.clone(),
            });
        },
        |statements, _, table_from, table_to| {
            diff_table(table_from, table_to, dialect, statements);
        },
        statements,
    );
}

pub(super) fn detect_table_renames(
    from: &IndexMap<Iden, Table>,
    to: &IndexMap<Iden, Table>,
) -> Vec<RenameScenario> {
    let created: Vec<_> = to
        .iter()
        .filter(|(name, _)| !from.contains_key(*name))
        .collect();
    let dropped: Vec<_> = from
        .iter()
        .filter(|(name, _)| !to.contains_key(*name))
        .collect();

    let dropped = dropped
        .iter()
        .map(|(_, table)| (table.name.clone(), RenameId::table(table_id(table))))
        .collect::<IndexMap<_, _>>();

    let created = created
        .iter()
        .map(|(_, table)| (table.name.clone(), RenameId::table(table_id(table))))
        .collect::<IndexMap<_, _>>();

    build_scenario(RenameKind::Table, None, created, dropped)
}

fn diff_table(from: &Table, to: &Table, dialect: &SqlDialect, statements: &mut Vec<DiffStatement>) {
    let schema = to.schema.clone();
    let table = to.name.clone();

    // Diff table comment
    if from.comment != to.comment {
        statements.push(DiffStatement::AlterTableComment {
            table: table.clone(),
            schema: schema.clone(),
            prev: from.comment.clone(),
            comment: to.comment.clone(),
        });
    }

    let option_changes = diff_table_options(&from.options, &to.options);

    if !option_changes.is_empty() {
        statements.push(DiffStatement::AlterTableOptions {
            table: table.clone(),
            schema: schema.clone(),
            changes: option_changes,
        });
    }

    if from.tablespace != to.tablespace {
        statements.push(DiffStatement::AlterTableTablespace {
            table: table.clone(),
            schema: schema.clone(),
            prev_tablespace: from.tablespace.clone(),
            tablespace: to.tablespace.clone(),
        });
    }

    if !json_eq(&from.partition, &to.partition) {
        statements.push(DiffStatement::AlterTablePartition {
            table: table.clone(),
            schema: schema.clone(),
            prev_partition: from.partition.clone(),
            partition: to.partition.clone(),
        });
    }

    // Diff columns
    diff_columns(
        &from.columns,
        &to.columns,
        dialect,
        &table,
        &schema,
        statements,
    );

    // Diff indexes
    diff_indexes(&from.indexes, &to.indexes, &table, &schema, statements);

    // Diff constraints
    diff_constraints(
        &from.constraints,
        &to.constraints,
        &table,
        &schema,
        statements,
    );
}

pub(super) fn detect_column_renames(
    from: &Table,
    to: &Table,
    require_same_table_name: bool,
) -> Vec<RenameScenario> {
    if require_same_table_name && from.name != to.name {
        return Vec::new();
    }
    let to_id = table_id(to);
    let dropped = from
        .columns
        .values()
        .filter(|column| !to.columns.contains_key(&column.name))
        .map(|column| {
            (
                column.name.clone(),
                RenameId::column(to_id.clone(), column.name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();
    let created = to
        .columns
        .values()
        .filter(|column| !from.columns.contains_key(&column.name))
        .map(|column| {
            (
                column.name.clone(),
                RenameId::column(to_id.clone(), column.name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();

    build_scenario(RenameKind::Column, Some(to_id), created, dropped)
}

pub(super) fn detect_index_renames(
    from: &Table,
    to: &Table,
    require_same_table_name: bool,
) -> Vec<RenameScenario> {
    if require_same_table_name && from.name != to.name {
        return Vec::new();
    }

    let table = table_id(to);
    let dropped = from
        .indexes
        .values()
        .filter(|index| !to.indexes.contains_key(&index.name))
        .map(|index| {
            (
                index.name.clone(),
                RenameId::index(table.clone(), index.name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();
    let created = to
        .indexes
        .values()
        .filter(|index| !from.indexes.contains_key(&index.name))
        .map(|index| {
            (
                index.name.clone(),
                RenameId::index(table.clone(), index.name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();

    build_scenario(RenameKind::Index, Some(table), created, dropped)
}

pub(super) fn detect_constraint_renames(
    from: &Table,
    to: &Table,
    require_same_table_name: bool,
) -> Vec<RenameScenario> {
    if require_same_table_name && from.name != to.name {
        return Vec::new();
    }

    let from_by_name: IndexMap<String, &Constraint> = from
        .constraints
        .iter()
        .filter_map(|c| c.name().map(|n| (n.to_owned(), c)))
        .collect();
    let to_by_name: IndexMap<String, &Constraint> = to
        .constraints
        .iter()
        .filter_map(|c| c.name().map(|n| (n.to_owned(), c)))
        .collect();

    let table = table_id(to);
    let dropped = from_by_name
        .keys()
        .filter(|name| !to_by_name.contains_key(*name))
        .map(|name| {
            (
                name.clone(),
                RenameId::constraint(table.clone(), name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();
    let created = to_by_name
        .keys()
        .filter(|name| !from_by_name.contains_key(*name))
        .map(|name| {
            (
                name.clone(),
                RenameId::constraint(table.clone(), name.clone()),
            )
        })
        .collect::<IndexMap<_, _>>();

    build_scenario(RenameKind::Constraint, Some(table), created, dropped)
}

fn diff_columns(
    from: &IndexMap<String, Column>,
    to: &IndexMap<String, Column>,
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, col_to| {
            statements.push(DiffStatement::AddColumn {
                table: table.to_string(),
                schema: schema.clone(),
                column: col_to.clone(),
            });
        },
        |statements, name, col_from| {
            statements.push(DiffStatement::DropColumn {
                table: table.to_string(),
                schema: schema.clone(),
                column: name.clone(),
                cascade: false,
                prev: col_from.clone(),
            });
        },
        |statements, name, col_from, col_to| {
            let changes = diff_column(col_from, col_to, dialect);
            if !changes.is_empty() {
                statements.push(DiffStatement::AlterColumn {
                    table: table.to_string(),
                    schema: schema.clone(),
                    column: name.clone(),
                    changes,
                });
            }

            // Check for comment changes (handled separately from other column changes)
            if col_from.comment != col_to.comment {
                statements.push(DiffStatement::AlterColumnComment {
                    table: table.to_string(),
                    schema: schema.clone(),
                    column: name.clone(),
                    comment: col_to.comment.clone(),
                    prev_comment: col_from.comment.clone(),
                });
            }
        },
        statements,
    );
}

fn diff_column(from: &Column, to: &Column, dialect: &SqlDialect) -> Vec<ColumnChange> {
    let mut changes = Vec::new();

    // Type change
    if from.data_type != to.data_type {
        changes.push(ColumnChange::SetType(to.data_type.to_string(dialect)));
    }

    // Nullability change
    if from.nullable && !to.nullable {
        changes.push(ColumnChange::SetNotNull);
    } else if !from.nullable && to.nullable {
        changes.push(ColumnChange::DropNotNull);
    }

    // Default change
    match (&from.default, &to.default) {
        (None, Some(d)) => changes.push(ColumnChange::SetDefault(d.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropDefault),
        (Some(d1), Some(d2))
            if normalize_default_expression(&d1.to_string())
                != normalize_default_expression(&d2.to_string()) =>
        {
            changes.push(ColumnChange::SetDefault(d2.clone()))
        }
        _ => {}
    }

    // Generated column change
    match (&from.generated, &to.generated) {
        (None, Some(g)) => changes.push(ColumnChange::SetGenerated(g.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropGenerated),
        (Some(g1), Some(g2)) if g1 != g2 => changes.push(ColumnChange::SetGenerated(g2.clone())),
        _ => {}
    }

    match (&from.collation, &to.collation) {
        (None, Some(collation)) => changes.push(ColumnChange::SetCollation(collation.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropCollation),
        (Some(from_collation), Some(to_collation)) if from_collation != to_collation => {
            changes.push(ColumnChange::SetCollation(to_collation.clone()))
        }
        _ => {}
    }

    match (&from.identity, &to.identity) {
        (None, Some(identity)) => changes.push(ColumnChange::SetIdentity(identity.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropIdentity),
        (Some(from_identity), Some(to_identity)) if !json_eq(from_identity, to_identity) => {
            changes.push(ColumnChange::DropIdentity);
            changes.push(ColumnChange::SetIdentity(to_identity.clone()));
        }
        _ => {}
    }

    changes
}

fn diff_table_options(
    from: &IndexMap<String, String>,
    to: &IndexMap<String, String>,
) -> Vec<super::TableOptionChange> {
    let mut changes = Vec::new();

    for (key, value) in to {
        match from.get(key) {
            Some(prev) if prev == value => {}
            prev => changes.push(super::TableOptionChange::Set {
                key: key.clone(),
                value: value.clone(),
                prev: prev.cloned(),
            }),
        }
    }

    for (key, prev) in from {
        if !to.contains_key(key) {
            changes.push(super::TableOptionChange::Drop {
                key: key.clone(),
                prev: prev.clone(),
            });
        }
    }

    changes
}

fn normalize_default_expression(expr: &str) -> String {
    let mut normalized = expr.trim().to_string();

    while has_wrapping_parentheses(&normalized) {
        normalized = normalized[1..normalized.len() - 1].trim().to_string();
    }

    if let Some((value, cast)) = normalized.rsplit_once("::") {
        let value = value.trim();
        let cast = cast.trim();
        if !cast.is_empty() && (is_quoted_literal(value) || is_scalar_literal(value)) {
            return value.to_string();
        }
    }

    normalized
}

fn has_wrapping_parentheses(expr: &str) -> bool {
    if !(expr.starts_with('(') && expr.ends_with(')')) {
        return false;
    }

    let inner = &expr[1..expr.len() - 1];
    let mut depth = 0_i32;
    for ch in inner.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    depth == 0
}

fn is_quoted_literal(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

fn is_scalar_literal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower == "null"
        || lower == "true"
        || lower == "false"
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
}

fn diff_indexes(
    from: &IndexMap<String, Index>,
    to: &IndexMap<String, Index>,
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, idx_to| {
            statements.push(DiffStatement::CreateIndex {
                table: table.to_string(),
                schema: schema.clone(),
                index: idx_to.clone(),
                concurrently: false,
                if_not_exists: false,
            });
        },
        |statements, name, idx_from| {
            statements.push(DiffStatement::DropIndex {
                table: table.to_string(),
                name: name.clone(),
                schema: schema.clone(),
                concurrently: false,
                if_exists: false,
                prev: idx_from.clone(),
            });
        },
        |statements, name, idx_from, idx_to| {
            if !json_eq(idx_from, idx_to) {
                // Drop and recreate
                statements.push(DiffStatement::DropIndex {
                    table: table.to_string(),
                    name: name.clone(),
                    schema: schema.clone(),
                    concurrently: false,
                    if_exists: false,
                    prev: idx_from.clone(),
                });
                statements.push(DiffStatement::CreateIndex {
                    table: table.to_string(),
                    schema: schema.clone(),
                    index: idx_to.clone(),
                    concurrently: false,
                    if_not_exists: false,
                });
            }
        },
        statements,
    );
}

fn diff_constraints(
    from: &[Constraint],
    to: &[Constraint],
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    // Build maps by name for named constraints
    let from_by_name: IndexMap<String, &Constraint> = from
        .iter()
        .filter_map(|c| c.name().map(|n| (n.to_owned(), c)))
        .collect();

    let to_by_name: IndexMap<String, &Constraint> = to
        .iter()
        .filter_map(|c| c.name().map(|n| (n.to_owned(), c)))
        .collect();

    // Constraints to add
    for (name, constraint) in &to_by_name {
        if !from_by_name.contains_key(name) {
            statements.push(DiffStatement::AddConstraint {
                table: table.to_string(),
                schema: schema.clone(),
                constraint: (*constraint).clone(),
            });
        }
    }

    // Constraints to drop
    for (name, constraint) in &from_by_name {
        if !to_by_name.contains_key(name) {
            statements.push(DiffStatement::DropConstraint {
                table: table.to_string(),
                schema: schema.clone(),
                name: name.clone(),
                cascade: false,
                prev: (*constraint).clone(),
            });
        }
    }

    // Constraints to modify (drop and recreate)
    for (name, constraint_to) in &to_by_name {
        if let Some(constraint_from) = from_by_name.get(name)
            && !json_eq(*constraint_from, *constraint_to)
        {
            statements.push(DiffStatement::DropConstraint {
                table: table.to_string(),
                schema: schema.clone(),
                name: name.clone(),
                cascade: false,
                prev: (*constraint_from).clone(),
            });
            statements.push(DiffStatement::AddConstraint {
                table: table.to_string(),
                schema: schema.clone(),
                constraint: (*constraint_to).clone(),
            });
        }
    }
}

pub(super) fn diff_views(
    from: &IndexMap<Iden, View>,
    to: &IndexMap<Iden, View>,
    statements: &mut Vec<DiffStatement>,
) {
    diff_index_map_entries(
        from,
        to,
        |statements, _, view_to| {
            statements.push(DiffStatement::CreateView {
                view: view_to.clone(),
                or_replace: false,
            });
        },
        |statements, _, view_from| {
            statements.push(DiffStatement::DropView {
                name: view_from.name.clone(),
                schema: view_from.schema.clone(),
                materialized: view_from.materialized,
                cascade: false,
                prev: view_from.clone(),
            });
        },
        |statements, _, view_from, view_to| {
            if view_from.materialized != view_to.materialized {
                statements.push(DiffStatement::DropView {
                    name: view_from.name.clone(),
                    schema: view_from.schema.clone(),
                    materialized: view_from.materialized,
                    cascade: false,
                    prev: view_from.clone(),
                });
                statements.push(DiffStatement::CreateView {
                    view: view_to.clone(),
                    or_replace: false,
                });
            } else if view_from.definition != view_to.definition {
                statements.push(DiffStatement::AlterView {
                    name: view_to.name.clone(),
                    schema: view_to.schema.clone(),
                    new_definition: view_to.definition.clone(),
                    prev_definition: view_from.definition.clone(),
                });
            }
        },
        statements,
    );
}

fn diff_string_entries<Include, OnAdd, OnDrop>(
    from: &[String],
    to: &[String],
    mut include: Include,
    mut on_add: OnAdd,
    mut on_drop: OnDrop,
    statements: &mut Vec<DiffStatement>,
) where
    Include: FnMut(&str) -> bool,
    OnAdd: FnMut(&mut Vec<DiffStatement>, &String),
    OnDrop: FnMut(&mut Vec<DiffStatement>, &String),
{
    let from_set: HashSet<&str> = from.iter().map(String::as_str).collect();
    let to_set: HashSet<&str> = to.iter().map(String::as_str).collect();

    for value in to {
        if include(value) && !from_set.contains(value.as_str()) {
            on_add(statements, value);
        }
    }

    for value in from {
        if include(value) && !to_set.contains(value.as_str()) {
            on_drop(statements, value);
        }
    }
}

fn diff_index_map_entries<K, V, OnAdd, OnDrop, OnShared>(
    from: &IndexMap<K, V>,
    to: &IndexMap<K, V>,
    mut on_add: OnAdd,
    mut on_drop: OnDrop,
    mut on_shared: OnShared,
    statements: &mut Vec<DiffStatement>,
) where
    K: Eq + Hash,
    OnAdd: FnMut(&mut Vec<DiffStatement>, &K, &V),
    OnDrop: FnMut(&mut Vec<DiffStatement>, &K, &V),
    OnShared: FnMut(&mut Vec<DiffStatement>, &K, &V, &V),
{
    for (key, to_value) in to {
        if !from.contains_key(key) {
            on_add(statements, key, to_value);
        }
    }

    for (key, from_value) in from {
        if !to.contains_key(key) {
            on_drop(statements, key, from_value);
        }
    }

    for (key, to_value) in to {
        if let Some(from_value) = from.get(key) {
            on_shared(statements, key, from_value, to_value);
        }
    }
}

fn json_eq<T: serde::Serialize>(left: &T, right: &T) -> bool {
    serde_json::to_value(left).ok() == serde_json::to_value(right).ok()
}

fn table_id(table: &Table) -> Iden {
    Iden::new(table.name.clone(), table.schema.clone())
}

fn build_scenario(
    kind: RenameKind,
    table: Option<Iden>,
    created: IndexMap<String, RenameId>,
    dropped: IndexMap<String, RenameId>,
) -> Vec<RenameScenario> {
    if created.is_empty() || dropped.is_empty() {
        Vec::new()
    } else {
        vec![RenameScenario::new(kind, table, created, dropped)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::schema::{
        DataType, DefaultValue, IdentitySpec, IndexColumn, IndexMethod, PartitionMethod,
        PartitionSpec, SequenceOptions, UniqueConstraint,
    };

    fn entity(name: &str, schema: Option<&str>) -> Iden {
        Iden::new(name, schema.map(str::to_string))
    }

    #[test]
    fn enum_addition_at_front_uses_before_position() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        from.insert(
            entity("status", Some("public")),
            DbEnum {
                name: "status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["published".to_string()],
                description: None,
            },
        );
        to.insert(
            entity("status", Some("public")),
            DbEnum {
                name: "status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["draft".to_string(), "published".to_string()],
                description: None,
            },
        );

        let mut statements = Vec::new();
        diff_enums(&from, &to, &mut statements);

        assert_eq!(statements.len(), 1);
        assert!(matches!(
            &statements[0],
            DiffStatement::AddEnumValue {
                value,
                position: EnumValuePosition::Before(next),
                ..
            } if value == "draft" && next == "published"
        ));
    }

    #[test]
    fn sequence_start_change_is_detected() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        from.insert(
            entity("order_seq", Some("public")),
            Sequence {
                name: "order_seq".to_string(),
                schema: Some("public".to_string()),
                start: 1,
                ..Default::default()
            },
        );
        to.insert(
            entity("order_seq", Some("public")),
            Sequence {
                name: "order_seq".to_string(),
                schema: Some("public".to_string()),
                start: 100,
                ..Default::default()
            },
        );

        let mut statements = Vec::new();
        diff_sequences(&from, &to, &mut statements);

        assert!(matches!(
            &statements[..],
            [DiffStatement::AlterSequence { changes, .. }]
                if matches!(&changes[..], [SequenceChange::Start(100)])
        ));
    }

    #[test]
    fn identical_sequences_do_not_emit_diff_statements() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        let sequence = Sequence {
            name: "order_seq".to_string(),
            schema: Some("public".to_string()),
            start: 1,
            increment: 1,
            min_value: 1,
            max_value: Some(2147483647),
            cache: 1,
            cycle: false,
        };

        from.insert(entity("order_seq", Some("public")), sequence.clone());
        to.insert(entity("order_seq", Some("public")), sequence);

        let mut statements = Vec::new();
        diff_sequences(&from, &to, &mut statements);

        assert!(statements.is_empty());
    }

    #[test]
    fn table_diffs_include_options_tablespace_and_partition() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        let from_table = Table::in_schema("users", "app")
            .option("fillfactor", "80")
            .tablespace("slowspace")
            .partition_by(PartitionMethod::Hash, vec!["tenant_id"]);
        let to_table = Table::in_schema("users", "app")
            .option("autovacuum_enabled", "false")
            .tablespace("fastspace")
            .partition_by(PartitionMethod::List, vec!["tenant_id"]);

        from.insert(entity("users", Some("app")), from_table);
        to.insert(entity("users", Some("app")), to_table);

        let mut statements = Vec::new();
        diff_tables(&from, &to, &SqlDialect::Postgres, &mut statements);

        assert!(matches!(
            &statements[0],
            DiffStatement::AlterTableOptions { changes, .. }
                if changes.len() == 2
        ));
        assert!(matches!(
            &statements[1],
            DiffStatement::AlterTableTablespace {
                prev_tablespace,
                tablespace,
                ..
            } if prev_tablespace.as_deref() == Some("slowspace")
                && tablespace.as_deref() == Some("fastspace")
        ));
        assert!(matches!(
            &statements[2],
            DiffStatement::AlterTablePartition {
                prev_partition: Some(PartitionSpec {
                    method: PartitionMethod::Hash,
                    ..
                }),
                partition: Some(PartitionSpec {
                    method: PartitionMethod::List,
                    ..
                }),
                ..
            }
        ));
    }

    #[test]
    fn column_diffs_include_collation_and_identity_changes() {
        let mut from = Column::new("email", DataType::Text).collate("en_US");
        from.identity = Some(IdentitySpec {
            always: false,
            sequence_options: Some(SequenceOptions {
                start: Some(1),
                ..Default::default()
            }),
        });

        let mut to = Column::new("email", DataType::Text).collate("de_DE");
        to.identity = Some(IdentitySpec {
            always: true,
            sequence_options: Some(SequenceOptions {
                start: Some(100),
                ..Default::default()
            }),
        });

        let changes = diff_column(&from, &to, &SqlDialect::Postgres);

        assert!(matches!(
            &changes[..],
            [
                ColumnChange::SetCollation(collation),
                ColumnChange::DropIdentity,
                ColumnChange::SetIdentity(IdentitySpec { always: true, .. })
            ] if collation == "de_DE"
        ));
    }

    #[test]
    fn equivalent_default_expressions_do_not_trigger_alter() {
        let from = Column::new("status", DataType::Text)
            .default(DefaultValue::Sql("(('draft'::text))".to_string()));
        let to =
            Column::new("status", DataType::Text).default(DefaultValue::Sql("'draft'".to_string()));

        let changes = diff_column(&from, &to, &SqlDialect::Postgres);

        assert!(changes.is_empty());
    }

    #[test]
    fn index_structural_change_recreates_index() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        from.insert(
            "users_email_idx".to_string(),
            Index::new("users_email_idx", vec!["email"]),
        );
        to.insert(
            "users_email_idx".to_string(),
            Index::with_columns("users_email_idx", vec![IndexColumn::column("email").desc()])
                .using(IndexMethod::Gin),
        );

        let mut statements = Vec::new();
        diff_indexes(
            &from,
            &to,
            "users",
            &Some("app".to_string()),
            &mut statements,
        );

        assert!(matches!(
            &statements[..],
            [
                DiffStatement::DropIndex { .. },
                DiffStatement::CreateIndex { .. }
            ]
        ));
    }

    #[test]
    fn constraint_structural_change_recreates_constraint() {
        let from = vec![Constraint::Unique(
            UniqueConstraint::new(vec!["email"]).named("users_email_key"),
        )];
        let to = vec![Constraint::Unique(
            UniqueConstraint::new(vec!["email"])
                .named("users_email_key")
                .nulls_not_distinct(),
        )];

        let mut statements = Vec::new();
        diff_constraints(
            &from,
            &to,
            "users",
            &Some("app".to_string()),
            &mut statements,
        );

        assert!(matches!(
            &statements[..],
            [
                DiffStatement::DropConstraint { .. },
                DiffStatement::AddConstraint { .. }
            ]
        ));
    }

    #[test]
    fn view_materialized_change_recreates_view() {
        let mut from = IndexMap::new();
        let mut to = IndexMap::new();

        from.insert(
            entity("active_users", Some("app")),
            View {
                name: "active_users".to_string(),
                schema: Some("app".to_string()),
                definition: "SELECT id FROM users".to_string(),
                materialized: false,
                columns: vec![],
            },
        );
        to.insert(
            entity("active_users", Some("app")),
            View {
                name: "active_users".to_string(),
                schema: Some("app".to_string()),
                definition: "SELECT id FROM users".to_string(),
                materialized: true,
                columns: vec![],
            },
        );

        let mut statements = Vec::new();
        diff_views(&from, &to, &mut statements);

        assert!(matches!(
            &statements[..],
            [
                DiffStatement::DropView { materialized: false, .. },
                DiffStatement::CreateView { view, .. }
            ] if view.materialized
        ));
    }
}
