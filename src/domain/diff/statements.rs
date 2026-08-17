use crate::models::iden::Iden;
use crate::schema::*;
use indexmap::IndexMap;

use super::rename::{RenameDecision, RenameId, RenameKind, RenameMap, RenameScenario};

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

    pub fn apply_rename_decisions(
        &self,
        decisions: &[RenameDecision],
    ) -> crate::Result<SchemaDiff> {
        if decisions.is_empty() {
            return Ok(self.clone());
        }

        for decision in decisions {
            self.validate_drop_decision(decision)?;
        }

        let mut renames = decisions
            .iter()
            .filter_map(|decision| match decision {
                RenameDecision::Rename(rename) => Some(rename),
                RenameDecision::Drop(_) => None,
            })
            .collect::<Vec<_>>();
        renames.sort_by_key(|rename| match rename.source.kind {
            RenameKind::Type => 0,
            RenameKind::Table => 1,
            RenameKind::Column => 2,
            RenameKind::Index => 3,
            RenameKind::Constraint => 4,
        });

        let mut statements = self.statements.clone();
        for rename in renames {
            self.validate_rename_decision(rename)?;
            let (matched_indices, additions) = replacement_for_rename(rename, &statements)?;
            Self::apply_statement_replacement(&mut statements, matched_indices, additions);
        }

        Ok(SchemaDiff {
            statements,
            rename_scenarios: self.rename_scenarios.clone(),
        })
    }

    fn apply_statement_replacement(
        statements: &mut Vec<DiffStatement>,
        mut removals: Vec<usize>,
        additions: Vec<DiffStatement>,
    ) {
        removals.sort_unstable();
        removals.dedup();
        let insert_at = removals[0];

        let mut next = Vec::with_capacity(statements.len() - removals.len() + additions.len());
        for (idx, stmt) in statements.iter().enumerate() {
            if idx == insert_at {
                next.extend(additions.iter().cloned());
            }

            if !removals.contains(&idx) {
                next.push(stmt.clone());
            }
        }

        *statements = next;
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

fn replacement_for_rename(
    rename: &RenameMap,
    statements: &[DiffStatement],
) -> crate::Result<(Vec<usize>, Vec<DiffStatement>)> {
    use crate::errors::ShkiError;

    let (mut removals, additions) = match rename.source.kind {
        RenameKind::Type => (
            vec![
                find_statement(statements, |stmt| matches_drop_type(stmt, &rename.source))?,
                find_statement(statements, |stmt| matches_create_type(stmt, &rename.target))?,
            ],
            vec![rename_statement(rename)?],
        ),
        RenameKind::Table => {
            let drop_idx =
                find_statement(statements, |stmt| matches_drop_table(stmt, &rename.source))?;
            let create_idx = find_statement(statements, |stmt| {
                matches_create_table(stmt, &rename.target)
            })?;
            let mut additions = vec![rename_statement(rename)?];
            additions.extend(nested_table_statements(
                &statements[drop_idx],
                &statements[create_idx],
            )?);
            // The created table also emitted standalone CreateIndex/AddConstraint
            // statements (see lower_table_diffs); nested_table_statements re-derives
            // the ones still needed, so remove the originals to avoid duplicates.
            let mut removals = vec![drop_idx, create_idx];
            removals.extend(statements.iter().enumerate().filter_map(|(idx, stmt)| {
                matches_table_child_creation(stmt, &rename.target).then_some(idx)
            }));
            (removals, additions)
        }
        RenameKind::Column => (
            vec![
                find_statement(statements, |stmt| matches_drop_column(stmt, &rename.source))?,
                find_statement(statements, |stmt| matches_add_column(stmt, &rename.target))?,
            ],
            vec![rename_statement(rename)?],
        ),
        RenameKind::Index => (
            vec![
                find_statement(statements, |stmt| matches_drop_index(stmt, &rename.source))?,
                find_statement(statements, |stmt| {
                    matches_create_index(stmt, &rename.target)
                })?,
            ],
            vec![rename_statement(rename)?],
        ),
        RenameKind::Constraint => (
            vec![
                find_statement(statements, |stmt| {
                    matches_drop_constraint(stmt, &rename.source)
                })?,
                find_statement(statements, |stmt| {
                    matches_add_constraint(stmt, &rename.target)
                })?,
            ],
            vec![rename_statement(rename)?],
        ),
    };

    removals.sort_unstable();
    removals.dedup();

    if removals.len() < 2 {
        return Err(ShkiError::diff(format!(
            "rename possibility no longer matches diff statements: {:?}",
            rename
        )));
    }

    Ok((removals, additions))
}

fn find_statement(
    statements: &[DiffStatement],
    matches: impl FnMut(&DiffStatement) -> bool,
) -> crate::Result<usize> {
    use crate::errors::ShkiError;

    statements
        .iter()
        .position(matches)
        .ok_or_else(|| ShkiError::diff("rename candidate no longer matches diff statements"))
}

fn nested_table_statements(
    drop_stmt: &DiffStatement,
    create_stmt: &DiffStatement,
) -> crate::Result<Vec<DiffStatement>> {
    use crate::errors::ShkiError;

    let DiffStatement::DropTable { prev: from, .. } = drop_stmt else {
        return Err(ShkiError::diff(
            "table rename source no longer matches diff statements",
        ));
    };
    let DiffStatement::CreateTable { table: to } = create_stmt else {
        return Err(ShkiError::diff(
            "table rename target no longer matches diff statements",
        ));
    };

    let mut statements = Vec::new();
    let table = to.name.clone();
    let schema = to.schema.clone();

    for column in to.columns.values() {
        if !from.columns.contains_key(&column.name) {
            statements.push(DiffStatement::AddColumn {
                table: table.clone(),
                schema: schema.clone(),
                column: column.clone(),
            });
        }
    }

    for (name, column) in &from.columns {
        if !to.columns.contains_key(name) {
            statements.push(DiffStatement::DropColumn {
                table: table.clone(),
                schema: schema.clone(),
                column: name.clone(),
                cascade: false,
                prev: column.clone(),
            });
        }
    }

    for index in to.indexes.values() {
        if !from.indexes.contains_key(&index.name) {
            statements.push(DiffStatement::CreateIndex {
                table: table.clone(),
                schema: schema.clone(),
                index: index.clone(),
                concurrently: false,
                if_not_exists: false,
            });
        }
    }

    for (name, index) in &from.indexes {
        if !to.indexes.contains_key(name) {
            statements.push(DiffStatement::DropIndex {
                table: table.clone(),
                schema: schema.clone(),
                name: name.clone(),
                concurrently: false,
                if_exists: false,
                prev: index.clone(),
            });
        }
    }

    let from_constraints = named_constraints_by_name(&from.constraints);
    let to_constraints = named_constraints_by_name(&to.constraints);

    for (name, constraint) in &to_constraints {
        if !from_constraints.contains_key(name) {
            statements.push(DiffStatement::AddConstraint {
                table: table.clone(),
                schema: schema.clone(),
                constraint: (*constraint).clone(),
            });
        }
    }

    for (name, constraint) in &from_constraints {
        if !to_constraints.contains_key(name) {
            statements.push(DiffStatement::DropConstraint {
                table: table.clone(),
                schema: schema.clone(),
                name: name.clone(),
                cascade: false,
                prev: (*constraint).clone(),
            });
        }
    }

    Ok(statements)
}

fn named_constraints_by_name(constraints: &[Constraint]) -> IndexMap<String, &Constraint> {
    constraints
        .iter()
        .filter_map(|constraint| constraint.name().map(|name| (name.to_owned(), constraint)))
        .collect()
}

fn rename_statement(rename: &RenameMap) -> crate::Result<DiffStatement> {
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

fn matches_drop_type(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::DropEnum { name, schema, .. } | DiffStatement::DropCompositeType { name, schema, .. } if name == &id.name && schema == &id.table.schema)
}

fn matches_create_type(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::CreateEnum { name, schema, .. } if name == &id.name && schema == &id.table.schema)
        || matches!(stmt, DiffStatement::CreateCompositeType { composite_type } if composite_type.name == id.name && composite_type.schema == id.table.schema)
}

fn matches_drop_table(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::DropTable { name, schema, .. } if name == &id.name && schema == &id.table.schema)
}

fn matches_create_table(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::CreateTable { table } if table.name == id.name && table.schema == id.table.schema)
}

fn matches_table_child_creation(stmt: &DiffStatement, id: &RenameId) -> bool {
    match stmt {
        DiffStatement::CreateIndex { table, schema, .. }
        | DiffStatement::AddConstraint { table, schema, .. } => {
            table == &id.name && schema == &id.table.schema
        }
        _ => false,
    }
}

fn matches_drop_column(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::DropColumn { table, schema, column, .. } if table == &id.table.name && schema == &id.table.schema && column == &id.name)
}

fn matches_add_column(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::AddColumn { table, schema, column } if table == &id.table.name && schema == &id.table.schema && column.name == id.name)
}

fn matches_drop_index(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::DropIndex { table, schema, name, .. } if table == &id.table.name && schema == &id.table.schema && name == &id.name)
}

fn matches_create_index(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::CreateIndex { table, schema, index, .. } if table == &id.table.name && schema == &id.table.schema && index.name == id.name)
}

fn matches_drop_constraint(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::DropConstraint { table, schema, name, .. } if table == &id.table.name && schema == &id.table.schema && name == &id.name)
}

fn matches_add_constraint(stmt: &DiffStatement, id: &RenameId) -> bool {
    matches!(stmt, DiffStatement::AddConstraint { table, schema, constraint } if table == &id.table.name && schema == &id.table.schema && constraint.name() == Some(id.name.as_str()))
}

impl SchemaDiff {
    fn validate_rename_decision(&self, rename: &RenameMap) -> crate::Result<()> {
        use crate::errors::ShkiError;

        if self.rename_scenarios.iter().any(|scenario| {
            scenario.kind == rename.source.kind
                && scenario.table_matches(&rename.source.table)
                && scenario.dropped.contains_key(&rename.source.name)
                && scenario.created.contains_key(&rename.target.name)
        }) {
            Ok(())
        } else {
            Err(ShkiError::diff(format!(
                "rename target not found in scenario: {:?}",
                rename
            )))
        }
    }

    fn validate_drop_decision(&self, decision: &RenameDecision) -> crate::Result<()> {
        use crate::errors::ShkiError;

        let RenameDecision::Drop(source) = decision else {
            return Ok(());
        };

        if self.rename_scenarios.iter().any(|scenario| {
            scenario.kind == source.kind
                && scenario.table_matches(&source.table)
                && scenario.dropped.contains_key(&source.name)
        }) {
            Ok(())
        } else {
            Err(ShkiError::diff(format!(
                "drop source not found in rename scenario: {:?}",
                source
            )))
        }
    }
}

impl RenameScenario {
    fn table_matches(&self, table: &Iden) -> bool {
        match &self.table {
            Some(scenario_table) => scenario_table == table,
            None => true,
        }
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
