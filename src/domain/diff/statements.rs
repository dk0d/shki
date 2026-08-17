use crate::models::iden::Iden;
use crate::schema::*;
use indexmap::IndexMap;

use super::rename::{RenameId, RenameKind, RenameMap, RenameScenario};

/// A diff between two schema snapshots
#[derive(Debug, Clone, Default)]
pub struct SchemaDiff {
    /// Statements to execute in order
    pub statements: Vec<DiffStatement>,
    /// Promptable add/drop scenarios that can be resolved as renames
    pub rename_scenarios: Vec<RenameScenario>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffSummary {
    pub schemas_created: Vec<String>,
    pub schemas_dropped: Vec<String>,
    pub schemas_renamed: Vec<String>,
    pub enums_created: Vec<String>,
    pub enums_dropped: Vec<String>,
    pub enums_renamed: Vec<String>,
    pub enums_altered: Vec<String>,
    pub enum_values_added: Vec<String>,
    pub sequences_created: Vec<String>,
    pub sequences_dropped: Vec<String>,
    pub sequences_altered: Vec<String>,
    pub tables_created: Vec<String>,
    pub tables_dropped: Vec<String>,
    pub tables_renamed: Vec<String>,
    pub tables_altered: Vec<String>,
    pub columns_added: Vec<String>,
    pub columns_dropped: Vec<String>,
    pub columns_renamed: Vec<String>,
    pub columns_altered: Vec<String>,
    pub indexes_created: Vec<String>,
    pub indexes_dropped: Vec<String>,
    pub indexes_renamed: Vec<String>,
    pub constraints_added: Vec<String>,
    pub constraints_dropped: Vec<String>,
    pub constraints_renamed: Vec<String>,
    pub views_created: Vec<String>,
    pub views_dropped: Vec<String>,
    pub views_altered: Vec<String>,
    pub extensions_created: Vec<String>,
    pub extensions_dropped: Vec<String>,
}

impl DiffSummary {
    fn push_unique(bucket: &mut Vec<String>, value: String) {
        if !bucket.contains(&value) {
            bucket.push(value);
        }
    }
}

impl SchemaDiff {
    /// Check if the diff is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }

    /// Get the number of statements
    pub fn len(&self) -> usize {
        self.statements.len()
    }

    pub fn has_rename_scenarios(&self) -> bool {
        self.rename_scenarios
            .iter()
            .any(|scenario| !scenario.dropped.is_empty())
    }

    /// Generate the reverse diff that undoes all changes
    ///
    /// Returns a new SchemaDiff with statements in reverse order that undo each change.
    /// Statements that cannot be reversed are omitted and listed in the second return value.
    ///
    /// # Returns
    /// A tuple of (reversible_diff, irreversible_statements)
    pub fn get_down_diff(&self) -> (SchemaDiff, Vec<DiffStatement>) {
        let mut reversible = Vec::new();
        let mut irreversible = Vec::new();

        // Process statements in reverse order
        for stmt in self.statements.iter().rev() {
            match stmt.undo_statement() {
                Some(reversed) => reversible.push(reversed),
                None => irreversible.push(stmt.clone()),
            }
        }

        (
            SchemaDiff {
                statements: reversible,
                rename_scenarios: Vec::new(),
            },
            irreversible,
        )
    }

    /// Check if all statements in this diff can be automatically reversed
    pub fn is_fully_reversible(&self) -> bool {
        self.statements.iter().all(|s| s.is_reversible())
    }

    /// Get the count of reversible and irreversible statements
    pub fn reversibility_stats(&self) -> (usize, usize) {
        let reversible = self.statements.iter().filter(|s| s.is_reversible()).count();
        let irreversible = self.statements.len() - reversible;
        (reversible, irreversible)
    }

    /// Check if the diff contains any destructive operations
    pub fn has_destructive_changes(&self) -> bool {
        self.statements.iter().any(|s| {
            matches!(
                s,
                DiffStatement::DropSchema { .. }
                    | DiffStatement::DropEnum { .. }
                    | DiffStatement::DropCompositeType { .. }
                    | DiffStatement::DropSequence { .. }
                    | DiffStatement::DropTable { .. }
                    | DiffStatement::DropColumn { .. }
                    | DiffStatement::DropView { .. }
            )
        })
    }

    /// Get a summary of changes
    pub fn summary(&self) -> DiffSummary {
        let mut summary = DiffSummary::default();

        for stmt in &self.statements {
            match stmt {
                DiffStatement::CreateSchema { name } => {
                    DiffSummary::push_unique(&mut summary.schemas_created, name.clone())
                }
                DiffStatement::DropSchema { name, .. } => {
                    DiffSummary::push_unique(&mut summary.schemas_dropped, name.clone())
                }
                DiffStatement::RenameSchema { from, to } => DiffSummary::push_unique(
                    &mut summary.schemas_renamed,
                    format!("{} -> {}", from, to),
                ),
                DiffStatement::CreateEnum { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.enums_created,
                    qualified_name(schema, name),
                ),
                DiffStatement::DropEnum { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.enums_dropped,
                    qualified_name(schema, name),
                ),
                DiffStatement::RenameType { from, to, schema } => DiffSummary::push_unique(
                    &mut summary.enums_renamed,
                    renamed_name(schema, from, to),
                ),
                DiffStatement::CreateCompositeType { composite_type } => DiffSummary::push_unique(
                    &mut summary.enums_created,
                    qualified_name(&composite_type.schema, &composite_type.name),
                ),
                DiffStatement::DropCompositeType { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.enums_dropped,
                    qualified_name(schema, name),
                ),
                DiffStatement::AddEnumValue {
                    enum_name,
                    schema,
                    value,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.enum_values_added,
                    qualified_child_name(schema, enum_name, value),
                ),
                DiffStatement::RenameEnumValue {
                    enum_name, schema, ..
                }
                | DiffStatement::DropEnumValue {
                    enum_name, schema, ..
                }
                | DiffStatement::ReorderEnumValues {
                    enum_name, schema, ..
                }
                | DiffStatement::AlterEnumDescription {
                    name: enum_name,
                    schema,
                    ..
                }
                | DiffStatement::RecreateEnum {
                    name: enum_name,
                    schema,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.enums_altered,
                    qualified_name(schema, enum_name),
                ),
                DiffStatement::CreateSequence { sequence } => DiffSummary::push_unique(
                    &mut summary.sequences_created,
                    qualified_name(&sequence.schema, &sequence.name),
                ),
                DiffStatement::DropSequence { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.sequences_dropped,
                    qualified_name(schema, name),
                ),
                DiffStatement::AlterSequence { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.sequences_altered,
                    qualified_name(schema, name),
                ),
                DiffStatement::CreateTable { table } => DiffSummary::push_unique(
                    &mut summary.tables_created,
                    qualified_name(&table.schema, &table.name),
                ),
                DiffStatement::DropTable { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.tables_dropped,
                    qualified_name(schema, name),
                ),
                DiffStatement::RenameTable { from, to, schema } => DiffSummary::push_unique(
                    &mut summary.tables_renamed,
                    renamed_name(schema, from, to),
                ),
                DiffStatement::AlterTableComment { table, schema, .. }
                | DiffStatement::AlterTableOptions { table, schema, .. }
                | DiffStatement::AlterTableTablespace { table, schema, .. }
                | DiffStatement::AlterTablePartition { table, schema, .. } => {
                    DiffSummary::push_unique(
                        &mut summary.tables_altered,
                        qualified_name(schema, table),
                    )
                }
                DiffStatement::AddColumn {
                    table,
                    schema,
                    column,
                } => DiffSummary::push_unique(
                    &mut summary.columns_added,
                    qualified_child_name(schema, table, &column.name),
                ),
                DiffStatement::DropColumn {
                    table,
                    schema,
                    column,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.columns_dropped,
                    qualified_child_name(schema, table, column),
                ),
                DiffStatement::RenameColumn {
                    table,
                    schema,
                    from,
                    to,
                } => DiffSummary::push_unique(
                    &mut summary.columns_renamed,
                    format!(
                        "{} -> {}",
                        qualified_child_name(schema, table, from),
                        qualified_child_name(schema, table, to)
                    ),
                ),
                DiffStatement::AlterColumn {
                    table,
                    schema,
                    column,
                    ..
                }
                | DiffStatement::AlterColumnComment {
                    table,
                    schema,
                    column,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.columns_altered,
                    qualified_child_name(schema, table, column),
                ),
                DiffStatement::CreateIndex {
                    table,
                    schema,
                    index,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.indexes_created,
                    qualified_grandchild_name(schema, table, &index.name),
                ),
                DiffStatement::DropIndex {
                    table,
                    schema,
                    name,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.indexes_dropped,
                    qualified_grandchild_name(schema, table, name),
                ),
                DiffStatement::RenameIndex {
                    table,
                    schema,
                    from,
                    to,
                } => DiffSummary::push_unique(
                    &mut summary.indexes_renamed,
                    format!(
                        "{} -> {}",
                        qualified_grandchild_name(schema, table, from),
                        qualified_grandchild_name(schema, table, to)
                    ),
                ),
                DiffStatement::AddConstraint {
                    table,
                    schema,
                    constraint,
                } => DiffSummary::push_unique(
                    &mut summary.constraints_added,
                    constraint_summary_name(schema, table, constraint.name()),
                ),
                DiffStatement::DropConstraint {
                    table,
                    schema,
                    name,
                    ..
                } => DiffSummary::push_unique(
                    &mut summary.constraints_dropped,
                    constraint_summary_name(schema, table, Some(name.as_str())),
                ),
                DiffStatement::RenameConstraint {
                    table,
                    schema,
                    from,
                    to,
                } => DiffSummary::push_unique(
                    &mut summary.constraints_renamed,
                    format!(
                        "{} -> {}",
                        constraint_summary_name(schema, table, Some(from.as_str())),
                        constraint_summary_name(schema, table, Some(to.as_str()))
                    ),
                ),
                DiffStatement::CreateView { view, .. } => DiffSummary::push_unique(
                    &mut summary.views_created,
                    qualified_name(&view.schema, &view.name),
                ),
                DiffStatement::DropView { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.views_dropped,
                    qualified_name(schema, name),
                ),
                DiffStatement::AlterView { name, schema, .. } => DiffSummary::push_unique(
                    &mut summary.views_altered,
                    qualified_name(schema, name),
                ),
                DiffStatement::CreateExtension(name) => {
                    DiffSummary::push_unique(&mut summary.extensions_created, name.clone())
                }
                DiffStatement::DropExtension(name) => {
                    DiffSummary::push_unique(&mut summary.extensions_dropped, name.clone())
                }
            }
        }

        summary
    }
}

/// A single diff statement
#[derive(Debug, Clone)]
pub enum DiffStatement {
    // Schema operations
    CreateSchema {
        name: String,
    },
    DropSchema {
        name: String,
        cascade: bool,
    },
    RenameSchema {
        from: String,
        to: String,
    },

    // Enum operations
    CreateEnum {
        name: String,
        schema: Option<String>,
        values: Vec<String>,
        description: Option<String>,
    },
    DropEnum {
        name: String,
        schema: Option<String>,
        prev: DbEnum,
    },
    RenameType {
        from: String,
        to: String,
        schema: Option<String>,
    },
    AddEnumValue {
        enum_name: String,
        schema: Option<String>,
        value: String,
        position: EnumValuePosition,
    },
    RenameEnumValue {
        enum_name: String,
        schema: Option<String>,
        from: String,
        to: String,
    },
    DropEnumValue {
        enum_name: String,
        schema: Option<String>,
        value: String,
    },
    ReorderEnumValues {
        enum_name: String,
        schema: Option<String>,
        values: Vec<String>,
        prev_values: Vec<String>,
    },
    RecreateEnum {
        name: String,
        schema: Option<String>,
        values: Vec<String>,
        prev: DbEnum,
        description: Option<String>,
    },
    AlterEnumDescription {
        name: String,
        schema: Option<String>,
        description: Option<String>,
        prev_description: Option<String>,
    },
    CreateCompositeType {
        composite_type: CompositeType,
    },
    DropCompositeType {
        name: String,
        schema: Option<String>,
        prev: CompositeType,
    },

    // Sequence operations
    CreateSequence {
        sequence: Sequence,
    },
    DropSequence {
        name: String,
        schema: Option<String>,
        prev: Sequence,
    },
    AlterSequence {
        name: String,
        schema: Option<String>,
        changes: Vec<SequenceChange>,
    },

    // Table operations
    CreateTable {
        table: Table,
    },
    DropTable {
        name: String,
        schema: Option<String>,
        cascade: bool,
        prev: Table,
    },
    RenameTable {
        from: String,
        to: String,
        schema: Option<String>,
    },
    AlterTableComment {
        table: String,
        schema: Option<String>,
        prev: Option<String>,
        comment: Option<String>,
    },
    AlterTableOptions {
        table: String,
        schema: Option<String>,
        changes: Vec<TableOptionChange>,
    },
    AlterTableTablespace {
        table: String,
        schema: Option<String>,
        prev_tablespace: Option<String>,
        tablespace: Option<String>,
    },
    AlterTablePartition {
        table: String,
        schema: Option<String>,
        prev_partition: Option<PartitionSpec>,
        partition: Option<PartitionSpec>,
    },

    // Column operations
    AddColumn {
        table: String,
        schema: Option<String>,
        column: Column,
    },
    DropColumn {
        table: String,
        schema: Option<String>,
        column: String,
        cascade: bool,
        prev: Column,
    },
    RenameColumn {
        table: String,
        schema: Option<String>,
        from: String,
        to: String,
    },
    AlterColumn {
        table: String,
        schema: Option<String>,
        column: String,
        changes: Vec<ColumnChange>,
    },
    AlterColumnComment {
        table: String,
        schema: Option<String>,
        column: String,
        comment: Option<String>,
        prev_comment: Option<String>,
    },

    // Index operations
    CreateIndex {
        table: String,
        schema: Option<String>,
        index: Index,
        concurrently: bool,
        if_not_exists: bool,
    },
    DropIndex {
        table: String,
        name: String,
        schema: Option<String>,
        concurrently: bool,
        if_exists: bool,
        prev: Index,
    },
    RenameIndex {
        table: String,
        schema: Option<String>,
        from: String,
        to: String,
    },

    // Constraint operations
    AddConstraint {
        table: String,
        schema: Option<String>,
        constraint: Constraint,
    },
    DropConstraint {
        table: String,
        schema: Option<String>,
        name: String,
        cascade: bool,
        prev: Constraint,
    },
    RenameConstraint {
        table: String,
        schema: Option<String>,
        from: String,
        to: String,
    },

    // View operations
    CreateView {
        view: View,
        or_replace: bool,
    },
    DropView {
        name: String,
        schema: Option<String>,
        materialized: bool,
        cascade: bool,
        prev: View,
    },
    AlterView {
        name: String,
        schema: Option<String>,
        new_definition: String,
        prev_definition: String,
    },

    // Extension operations (PostgreSQL)
    CreateExtension(String),
    DropExtension(String),
}

// Supporting enums for statement fields
#[derive(Debug, Clone)]
pub enum EnumValuePosition {
    End,
    Before(String),
    After(String),
}

#[derive(Debug, Clone)]
pub enum SequenceChange {
    Increment(i64),
    MinValue(i64),
    MaxValue(Option<i64>),
    Start(i64),
    Cache(i64),
    Cycle(bool),
}

#[derive(Debug, Clone)]
pub enum ColumnChange {
    SetType(String),
    SetNotNull,
    DropNotNull,
    SetDefault(DefaultValue),
    DropDefault,
    SetGenerated(GeneratedColumn),
    DropGenerated,
    SetCollation(String),
    DropCollation,
    SetIdentity(IdentitySpec),
    DropIdentity,
    AlterIdentity(Vec<IdentityChange>),
}

#[derive(Debug, Clone)]
pub enum IdentityChange {
    SetGeneratedAlways,
    SetGeneratedByDefault,
    SetSequenceOptions(SequenceOptions),
    DropSequenceOptions,
}

#[derive(Debug, Clone)]
pub enum TableOptionChange {
    Set {
        key: String,
        value: String,
        prev: Option<String>,
    },
    Drop {
        key: String,
        prev: String,
    },
}

impl DiffStatement {
    pub fn is_reversible(&self) -> bool {
        self.undo_statement().is_some()
    }

    pub fn undo_statement(&self) -> Option<DiffStatement> {
        match self {
            DiffStatement::CreateSchema { name } => Some(DiffStatement::DropSchema {
                name: name.clone(),
                cascade: false,
            }),
            DiffStatement::DropSchema { name, .. } => {
                Some(DiffStatement::CreateSchema { name: name.clone() })
            }
            DiffStatement::RenameSchema { from, to } => Some(DiffStatement::RenameSchema {
                from: to.clone(),
                to: from.clone(),
            }),
            DiffStatement::CreateEnum {
                name,
                schema,
                values,
                description,
            } => Some(DiffStatement::DropEnum {
                name: name.clone(),
                schema: schema.clone(),
                prev: DbEnum {
                    name: name.clone(),
                    schema: schema.clone(),
                    values: values.clone(),
                    description: description.clone(),
                },
            }),
            DiffStatement::DropEnum { prev, .. } => Some(DiffStatement::CreateEnum {
                name: prev.name.clone(),
                schema: prev.schema.clone(),
                values: prev.values.clone(),
                description: prev.description.clone(),
            }),
            DiffStatement::RenameType { from, to, schema } => Some(DiffStatement::RenameType {
                from: to.clone(),
                to: from.clone(),
                schema: schema.clone(),
            }),
            DiffStatement::AddEnumValue {
                enum_name,
                schema,
                value,
                ..
            } => Some(DiffStatement::DropEnumValue {
                enum_name: enum_name.clone(),
                schema: schema.clone(),
                value: value.clone(),
            }),
            DiffStatement::RenameEnumValue {
                enum_name,
                schema,
                from,
                to,
            } => Some(DiffStatement::RenameEnumValue {
                enum_name: enum_name.clone(),
                schema: schema.clone(),
                from: to.clone(),
                to: from.clone(),
            }),
            DiffStatement::DropEnumValue { .. } => None,
            DiffStatement::ReorderEnumValues {
                enum_name,
                schema,
                values,
                prev_values,
            } => Some(DiffStatement::ReorderEnumValues {
                enum_name: enum_name.clone(),
                schema: schema.clone(),
                values: prev_values.clone(),
                prev_values: values.clone(),
            }),
            DiffStatement::RecreateEnum {
                name,
                schema,
                values,
                prev,
                ..
            } => Some(DiffStatement::RecreateEnum {
                name: name.clone(),
                schema: schema.clone(),
                values: prev.values.clone(),
                prev: DbEnum {
                    name: name.clone(),
                    schema: schema.clone(),
                    values: values.clone(),
                    description: prev.description.clone(),
                },
                description: prev.description.clone(),
            }),
            DiffStatement::AlterEnumDescription {
                name,
                schema,
                description,
                prev_description,
            } => Some(DiffStatement::AlterEnumDescription {
                name: name.clone(),
                schema: schema.clone(),
                description: prev_description.clone(),
                prev_description: description.clone(),
            }),
            DiffStatement::CreateCompositeType { composite_type } => {
                Some(DiffStatement::DropCompositeType {
                    name: composite_type.name.clone(),
                    schema: composite_type.schema.clone(),
                    prev: composite_type.clone(),
                })
            }
            DiffStatement::DropCompositeType { prev, .. } => {
                Some(DiffStatement::CreateCompositeType {
                    composite_type: prev.clone(),
                })
            }
            DiffStatement::CreateSequence { sequence } => Some(DiffStatement::DropSequence {
                name: sequence.name.clone(),
                schema: sequence.schema.clone(),
                prev: sequence.clone(),
            }),
            DiffStatement::DropSequence { prev, .. } => Some(DiffStatement::CreateSequence {
                sequence: prev.clone(),
            }),
            DiffStatement::AlterSequence { .. } => None,
            DiffStatement::CreateTable { table } => Some(DiffStatement::DropTable {
                name: table.name.clone(),
                schema: table.schema.clone(),
                cascade: false,
                prev: table.clone(),
            }),
            DiffStatement::DropTable { prev, .. } => Some(DiffStatement::CreateTable {
                table: prev.clone(),
            }),
            DiffStatement::RenameTable { from, to, schema } => Some(DiffStatement::RenameTable {
                from: to.clone(),
                to: from.clone(),
                schema: schema.clone(),
            }),
            DiffStatement::AlterTableComment {
                table,
                schema,
                prev,
                comment,
            } => Some(DiffStatement::AlterTableComment {
                table: table.clone(),
                schema: schema.clone(),
                prev: comment.clone(),
                comment: prev.clone(),
            }),
            DiffStatement::AlterTableOptions {
                table,
                schema,
                changes,
            } => {
                let reversed = changes
                    .iter()
                    .rev()
                    .map(|change| match change {
                        TableOptionChange::Set { key, prev, .. } => match prev {
                            Some(prev) => TableOptionChange::Set {
                                key: key.clone(),
                                value: prev.clone(),
                                prev: None,
                            },
                            None => TableOptionChange::Drop {
                                key: key.clone(),
                                prev: String::new(),
                            },
                        },
                        TableOptionChange::Drop { key, prev } => TableOptionChange::Set {
                            key: key.clone(),
                            value: prev.clone(),
                            prev: None,
                        },
                    })
                    .collect();

                Some(DiffStatement::AlterTableOptions {
                    table: table.clone(),
                    schema: schema.clone(),
                    changes: reversed,
                })
            }
            DiffStatement::AlterTableTablespace {
                table,
                schema,
                prev_tablespace,
                tablespace,
            } => Some(DiffStatement::AlterTableTablespace {
                table: table.clone(),
                schema: schema.clone(),
                prev_tablespace: tablespace.clone(),
                tablespace: prev_tablespace.clone(),
            }),
            DiffStatement::AlterTablePartition {
                table,
                schema,
                prev_partition,
                partition,
            } => Some(DiffStatement::AlterTablePartition {
                table: table.clone(),
                schema: schema.clone(),
                prev_partition: partition.clone(),
                partition: prev_partition.clone(),
            }),
            DiffStatement::AddColumn {
                table,
                schema,
                column,
            } => Some(DiffStatement::DropColumn {
                table: table.clone(),
                schema: schema.clone(),
                column: column.name.clone(),
                cascade: false,
                prev: column.clone(),
            }),
            DiffStatement::DropColumn {
                table,
                schema,
                prev,
                ..
            } => Some(DiffStatement::AddColumn {
                table: table.clone(),
                schema: schema.clone(),
                column: prev.clone(),
            }),
            DiffStatement::RenameColumn {
                table,
                schema,
                from,
                to,
            } => Some(DiffStatement::RenameColumn {
                table: table.clone(),
                schema: schema.clone(),
                from: to.clone(),
                to: from.clone(),
            }),
            DiffStatement::AlterColumn { .. } => None,
            DiffStatement::AlterColumnComment {
                table,
                schema,
                column,
                comment,
                prev_comment,
            } => Some(DiffStatement::AlterColumnComment {
                table: table.clone(),
                schema: schema.clone(),
                column: column.clone(),
                comment: prev_comment.clone(),
                prev_comment: comment.clone(),
            }),
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
                concurrently,
                if_not_exists,
            } => Some(DiffStatement::DropIndex {
                table: table.clone(),
                name: index.name.clone(),
                schema: schema.clone(),
                concurrently: *concurrently,
                if_exists: *if_not_exists,
                prev: index.clone(),
            }),
            DiffStatement::DropIndex {
                table,
                schema,
                concurrently,
                if_exists,
                prev,
                ..
            } => Some(DiffStatement::CreateIndex {
                table: table.clone(),
                schema: schema.clone(),
                index: prev.clone(),
                concurrently: *concurrently,
                if_not_exists: *if_exists,
            }),
            DiffStatement::RenameIndex {
                table,
                schema,
                from,
                to,
            } => Some(DiffStatement::RenameIndex {
                table: table.clone(),
                schema: schema.clone(),
                from: to.clone(),
                to: from.clone(),
            }),
            DiffStatement::AddConstraint {
                table,
                schema,
                constraint,
            } => Some(DiffStatement::DropConstraint {
                table: table.clone(),
                schema: schema.clone(),
                name: constraint.name()?.to_string(),
                cascade: false,
                prev: constraint.clone(),
            }),
            DiffStatement::DropConstraint {
                table,
                schema,
                prev,
                ..
            } => Some(DiffStatement::AddConstraint {
                table: table.clone(),
                schema: schema.clone(),
                constraint: prev.clone(),
            }),
            DiffStatement::RenameConstraint {
                table,
                schema,
                from,
                to,
            } => Some(DiffStatement::RenameConstraint {
                table: table.clone(),
                schema: schema.clone(),
                from: to.clone(),
                to: from.clone(),
            }),
            DiffStatement::CreateView { view, .. } => Some(DiffStatement::DropView {
                name: view.name.clone(),
                schema: view.schema.clone(),
                materialized: view.materialized,
                cascade: false,
                prev: view.clone(),
            }),
            DiffStatement::DropView { prev, .. } => Some(DiffStatement::CreateView {
                view: prev.clone(),
                or_replace: false,
            }),
            DiffStatement::AlterView {
                name,
                schema,
                new_definition,
                prev_definition,
            } => Some(DiffStatement::AlterView {
                name: name.clone(),
                schema: schema.clone(),
                new_definition: prev_definition.clone(),
                prev_definition: new_definition.clone(),
            }),
            DiffStatement::CreateExtension(name) => {
                Some(DiffStatement::DropExtension(name.clone()))
            }
            DiffStatement::DropExtension(name) => {
                Some(DiffStatement::CreateExtension(name.clone()))
            }
        }
    }
}

impl RenameScenario {
    pub(super) fn new(
        kind: RenameKind,
        table: Option<Iden>,
        created: IndexMap<String, RenameId>,
        dropped: IndexMap<String, RenameId>,
    ) -> Self {
        Self {
            kind,
            table,
            created,
            dropped,
        }
    }
}

pub(crate) fn rename_statement(rename: &RenameMap) -> crate::Result<DiffStatement> {
    match rename.source.kind {
        RenameKind::Type => Ok(DiffStatement::RenameType {
            from: rename.source.name.clone(),
            to: rename.target.name.clone(),
            schema: rename.source.table.schema.clone(),
        }),
        RenameKind::Table => Ok(DiffStatement::RenameTable {
            from: rename.source.name.clone(),
            to: rename.target.name.clone(),
            schema: rename.source.table.schema.clone(),
        }),
        RenameKind::Column => Ok(DiffStatement::RenameColumn {
            table: rename.source.table.name.clone(),
            schema: rename.source.table.schema.clone(),
            from: rename.source.name.clone(),
            to: rename.target.name.clone(),
        }),
        RenameKind::Index => Ok(DiffStatement::RenameIndex {
            table: rename.source.table.name.clone(),
            schema: rename.source.table.schema.clone(),
            from: rename.source.name.clone(),
            to: rename.target.name.clone(),
        }),
        RenameKind::Constraint => Ok(DiffStatement::RenameConstraint {
            table: rename.source.table.name.clone(),
            schema: rename.source.table.schema.clone(),
            from: rename.source.name.clone(),
            to: rename.target.name.clone(),
        }),
    }
}

impl std::fmt::Display for ColumnChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnChange::SetType(data_type) => write!(f, "set type {}", data_type),
            ColumnChange::SetNotNull => write!(f, "set not null"),
            ColumnChange::DropNotNull => write!(f, "drop not null"),
            ColumnChange::SetDefault(default) => write!(f, "set default {}", default),
            ColumnChange::DropDefault => write!(f, "drop default"),
            ColumnChange::SetGenerated(expr) => write!(f, "set generated {}", expr),
            ColumnChange::DropGenerated => write!(f, "drop generated"),
            ColumnChange::SetCollation(collation) => write!(f, "set collation {}", collation),
            ColumnChange::DropCollation => write!(f, "drop collation"),
            ColumnChange::SetIdentity(identity) => write!(
                f,
                "set identity {}",
                if identity.always {
                    "always"
                } else {
                    "by default"
                }
            ),
            ColumnChange::DropIdentity => write!(f, "drop identity"),
            ColumnChange::AlterIdentity(changes) => {
                let rendered = changes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "alter identity {}", rendered)
            }
        }
    }
}

impl std::fmt::Display for IdentityChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityChange::SetGeneratedAlways => write!(f, "set generated always"),
            IdentityChange::SetGeneratedByDefault => write!(f, "set generated by default"),
            IdentityChange::SetSequenceOptions(options) => write!(
                f,
                "set sequence options {}",
                format_sequence_options(options)
            ),
            IdentityChange::DropSequenceOptions => write!(f, "drop sequence options"),
        }
    }
}

impl std::fmt::Display for TableOptionChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableOptionChange::Set { key, value, .. } => {
                write!(f, "set option {}={}", key, value)
            }
            TableOptionChange::Drop { key, .. } => write!(f, "drop option {}", key),
        }
    }
}

fn format_sequence_options(options: &SequenceOptions) -> String {
    let mut parts = Vec::new();

    if let Some(start) = options.start {
        parts.push(format!("start {}", start));
    }
    if let Some(increment) = options.increment {
        parts.push(format!("increment {}", increment));
    }
    if let Some(min_value) = options.min_value {
        parts.push(format!("minvalue {}", min_value));
    }
    if let Some(max_value) = options.max_value {
        parts.push(format!("maxvalue {}", max_value));
    }
    if let Some(cache) = options.cache {
        parts.push(format!("cache {}", cache));
    }
    if options.cycle {
        parts.push("cycle".to_string());
    }

    if parts.is_empty() {
        "default".to_string()
    } else {
        parts.join(" ")
    }
}

fn qualified_name(schema: &Option<String>, name: &str) -> String {
    match schema {
        Some(schema) => format!("{}.{}", schema, name),
        None => name.to_string(),
    }
}

fn qualified_child_name(schema: &Option<String>, parent: &str, child: &str) -> String {
    format!("{}.{}", qualified_name(schema, parent), child)
}

fn qualified_grandchild_name(schema: &Option<String>, parent: &str, child: &str) -> String {
    format!("{}.{}", qualified_name(schema, parent), child)
}

fn renamed_name(schema: &Option<String>, from: &str, to: &str) -> String {
    format!(
        "{} -> {}",
        qualified_name(schema, from),
        qualified_name(schema, to)
    )
}

fn constraint_summary_name(schema: &Option<String>, table: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => qualified_child_name(schema, table, name),
        None => qualified_child_name(schema, table, "<unnamed>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Constraint, PrimaryKeyConstraint};

    #[test]
    fn displays_column_collation_changes() {
        assert_eq!(
            ColumnChange::SetCollation("en_US".to_string()).to_string(),
            "set collation en_US"
        );
        assert_eq!(ColumnChange::DropCollation.to_string(), "drop collation");
    }

    #[test]
    fn displays_column_identity_changes() {
        assert_eq!(
            ColumnChange::SetIdentity(IdentitySpec {
                always: true,
                sequence_options: None,
            })
            .to_string(),
            "set identity always"
        );
        assert_eq!(ColumnChange::DropIdentity.to_string(), "drop identity");
        assert_eq!(
            ColumnChange::AlterIdentity(vec![
                IdentityChange::SetGeneratedByDefault,
                IdentityChange::SetSequenceOptions(SequenceOptions {
                    start: Some(100),
                    increment: Some(10),
                    ..Default::default()
                }),
            ])
            .to_string(),
            "alter identity set generated by default, set sequence options start 100 increment 10"
        );
    }

    #[test]
    fn supports_non_additive_enum_changes() {
        let prev = DbEnum {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            values: vec!["draft".to_string(), "published".to_string()],
            description: Some("state".to_string()),
        };

        let rename = DiffStatement::RenameEnumValue {
            enum_name: "status".to_string(),
            schema: Some("public".to_string()),
            from: "draft".to_string(),
            to: "pending".to_string(),
        };
        let drop_value = DiffStatement::DropEnumValue {
            enum_name: "status".to_string(),
            schema: Some("public".to_string()),
            value: "draft".to_string(),
        };
        let reorder = DiffStatement::ReorderEnumValues {
            enum_name: "status".to_string(),
            schema: Some("public".to_string()),
            values: vec!["published".to_string(), "draft".to_string()],
            prev_values: prev.values.clone(),
        };
        let recreate = DiffStatement::RecreateEnum {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            values: vec!["pending".to_string(), "published".to_string()],
            prev,
            description: Some("state".to_string()),
        };

        assert!(matches!(rename, DiffStatement::RenameEnumValue { .. }));
        assert!(matches!(drop_value, DiffStatement::DropEnumValue { .. }));
        assert!(matches!(reorder, DiffStatement::ReorderEnumValues { .. }));
        assert!(matches!(recreate, DiffStatement::RecreateEnum { .. }));
    }

    #[test]
    fn builds_down_diff_and_tracks_irreversible_statements() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "analytics".to_string(),
                },
                DiffStatement::AlterSequence {
                    name: "seq".to_string(),
                    schema: Some("public".to_string()),
                    changes: vec![SequenceChange::Increment(2)],
                },
                DiffStatement::AddConstraint {
                    table: "users".to_string(),
                    schema: Some("app".to_string()),
                    constraint: Constraint::PrimaryKey(
                        PrimaryKeyConstraint::new(vec!["id"]).named("users_pkey"),
                    ),
                },
            ],
            rename_scenarios: Vec::new(),
        };

        let (down, irreversible) = diff.get_down_diff();

        assert_eq!(down.len(), 2);
        assert_eq!(irreversible.len(), 1);
        assert!(matches!(
            &down.statements[0],
            DiffStatement::DropConstraint { name, .. } if name == "users_pkey"
        ));
        assert!(matches!(
            &down.statements[1],
            DiffStatement::DropSchema { name, cascade: false } if name == "analytics"
        ));
        assert!(matches!(
            &irreversible[0],
            DiffStatement::AlterSequence { name, .. } if name == "seq"
        ));
        assert!(!diff.is_fully_reversible());
        assert_eq!(diff.reversibility_stats(), (2, 1));
    }

    #[test]
    fn summarizes_and_flags_destructive_changes() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "analytics".to_string(),
                },
                DiffStatement::DropSchema {
                    name: "legacy".to_string(),
                    cascade: false,
                },
                DiffStatement::AddEnumValue {
                    enum_name: "status".to_string(),
                    schema: Some("public".to_string()),
                    value: "archived".to_string(),
                    position: EnumValuePosition::End,
                },
                DiffStatement::CreateExtension("pgcrypto".to_string()),
                DiffStatement::DropExtension("citext".to_string()),
            ],
            rename_scenarios: Vec::new(),
        };

        let summary = diff.summary();

        assert_eq!(summary.schemas_created, vec!["analytics".to_string()]);
        assert_eq!(summary.schemas_dropped, vec!["legacy".to_string()]);
        assert_eq!(
            summary.enum_values_added,
            vec!["public.status.archived".to_string()]
        );
        assert_eq!(summary.extensions_created, vec!["pgcrypto".to_string()]);
        assert_eq!(summary.extensions_dropped, vec!["citext".to_string()]);
        assert!(diff.has_destructive_changes());
    }

    #[test]
    fn summary_collects_unique_changed_names() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::AlterTableComment {
                    table: "users".to_string(),
                    schema: Some("app".to_string()),
                    prev: None,
                    comment: Some("hello".to_string()),
                },
                DiffStatement::AlterTableOptions {
                    table: "users".to_string(),
                    schema: Some("app".to_string()),
                    changes: vec![TableOptionChange::Set {
                        key: "fillfactor".to_string(),
                        value: "80".to_string(),
                        prev: None,
                    }],
                },
                DiffStatement::RenameColumn {
                    table: "users".to_string(),
                    schema: Some("app".to_string()),
                    from: "email".to_string(),
                    to: "primary_email".to_string(),
                },
            ],
            rename_scenarios: Vec::new(),
        };

        let summary = diff.summary();

        assert_eq!(summary.tables_altered, vec!["app.users".to_string()]);
        assert_eq!(
            summary.columns_renamed,
            vec!["app.users.email -> app.users.primary_email".to_string()]
        );
    }
}
