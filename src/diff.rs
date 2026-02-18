//! Schema diffing algorithm
//!
//! This module computes the difference between two schema snapshots and produces
//! a list of statements needed to migrate from one to the other.
//!
//! The basic diff produces DropTable/CreateTable, DropColumn/AddColumn, and
//! DropEnum/CreateEnum statements. To detect and apply renames, use the
//! `RenameDetector` from the rename module to analyze the diff and transform
//! these into RenameTable/RenameColumn/RenameEnum statements.

use indexmap::IndexMap;

use crate::Result;
use crate::schema::SchemaDialect;
use crate::snapshot::{
    ColumnSnapshot, ConstraintSnapshot, EnumSnapshot, IndexSnapshot, SequenceSnapshot, Snapshot,
    TableSnapshot, ViewSnapshot,
};

/// A diff between two schema snapshots
#[derive(Debug, Clone, Default)]
pub struct SchemaDiff {
    /// Statements to execute in order
    pub statements: Vec<DiffStatement>,
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
        prev: EnumSnapshot,
    },
    RenameEnum {
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
    AlterEnumDescription {
        name: String,
        schema: Option<String>,
        description: Option<String>,
        prev_description: Option<String>,
    },

    // Sequence operations
    CreateSequence {
        sequence: SequenceSnapshot,
    },
    DropSequence {
        name: String,
        schema: Option<String>,
        prev: SequenceSnapshot,
    },
    AlterSequence {
        name: String,
        schema: Option<String>,
        changes: Vec<SequenceChange>,
    },

    // Table operations
    CreateTable {
        table: TableSnapshot,
    },
    DropTable {
        name: String,
        schema: Option<String>,
        cascade: bool,
        prev: TableSnapshot,
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

    // Column operations
    AddColumn {
        table: String,
        schema: Option<String>,
        column: ColumnSnapshot,
    },
    DropColumn {
        table: String,
        schema: Option<String>,
        column: String,
        cascade: bool,
        prev: ColumnSnapshot,
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
        index: IndexSnapshot,
        concurrently: bool,
        if_not_exists: bool,
    },
    DropIndex {
        table: String,
        name: String,
        schema: Option<String>,
        concurrently: bool,
        if_exists: bool,
        prev: IndexSnapshot,
    },

    // Constraint operations
    AddConstraint {
        table: String,
        schema: Option<String>,
        constraint: ConstraintSnapshot,
    },
    DropConstraint {
        table: String,
        schema: Option<String>,
        name: String,
        cascade: bool,
        prev: ConstraintSnapshot,
    },

    // View operations
    CreateView {
        view: ViewSnapshot,
        or_replace: bool,
    },
    DropView {
        name: String,
        schema: Option<String>,
        materialized: bool,
        cascade: bool,
        prev: ViewSnapshot,
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

impl DiffStatement {
    /// Generate the reverse statement that undoes this change
    ///
    /// Note: Some operations cannot be perfectly reversed:
    /// - AddEnumValue cannot be reversed (PostgreSQL doesn't support removing enum values)
    /// - AlterColumn changes may lose information about the original state
    /// - AlterEnumDescription, AlterTableComment, AlterColumnComment need the original value
    ///
    /// For operations that can't be perfectly reversed, this returns None.
    pub fn undo_statement(&self) -> Option<DiffStatement> {
        match self {
            // Schema operations
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

            // Enum operations
            DiffStatement::CreateEnum {
                name,
                schema,
                values,
                description,
            } => Some(DiffStatement::DropEnum {
                name: name.clone(),
                schema: schema.clone(),
                prev: EnumSnapshot {
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
            DiffStatement::RenameEnum { from, to, schema } => Some(DiffStatement::RenameEnum {
                from: to.clone(),
                to: from.clone(),
                schema: schema.clone(),
            }),
            DiffStatement::AddEnumValue { .. } => {
                // PostgreSQL doesn't support removing enum values
                None
            }
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

            // Sequence operations
            DiffStatement::CreateSequence { sequence } => Some(DiffStatement::DropSequence {
                name: sequence.name.clone(),
                schema: sequence.schema.clone(),
                prev: sequence.clone(),
            }),
            DiffStatement::DropSequence { prev, .. } => Some(DiffStatement::CreateSequence {
                sequence: prev.clone(),
            }),
            DiffStatement::AlterSequence { .. } => {
                // Cannot know the previous values
                None
            }

            // Table operations
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

            // Column operations
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
            DiffStatement::AlterColumn { .. } => {
                // Cannot know the previous column state
                None
            }
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

            // Index operations
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
                ..
            } => Some(DiffStatement::DropIndex {
                table: table.clone(),
                name: index.name.clone(),
                schema: schema.clone(),
                concurrently: false,
                if_exists: false,
                prev: index.clone(),
            }),
            DiffStatement::DropIndex {
                table,
                schema,
                prev,
                ..
            } => Some(DiffStatement::CreateIndex {
                table: table.clone(),
                schema: schema.clone(),
                index: prev.clone(),
                concurrently: false,
                if_not_exists: false,
            }),

            // Constraint operations
            DiffStatement::AddConstraint {
                table,
                schema,
                constraint,
            } => {
                // Can only drop named constraints
                constraint
                    .name
                    .as_ref()
                    .map(|name| DiffStatement::DropConstraint {
                        table: table.clone(),
                        schema: schema.clone(),
                        name: name.clone(),
                        cascade: false,
                        prev: constraint.clone(),
                    })
            }
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

            // View operations
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

            // Extension operations
            DiffStatement::CreateExtension(name) => {
                Some(DiffStatement::DropExtension(name.clone()))
            }
            DiffStatement::DropExtension(name) => {
                Some(DiffStatement::CreateExtension(name.clone()))
            }
        }
    }

    /// Check if this statement can be automatically reversed
    pub fn is_reversible(&self) -> bool {
        self.undo_statement().is_some()
    }
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
    SetDefault(String),
    DropDefault,
    SetGenerated(String),
    DropGenerated,
}

/// Compute the diff between two snapshots
///
/// This produces a basic diff with DropTable/CreateTable, DropColumn/AddColumn,
/// and DropEnum/CreateEnum statements. To convert these into rename statements,
/// use `RenameDetector` to detect potential renames, then `apply_renames` to
/// transform the diff.
pub fn diff_snapshots(from: &Snapshot, to: &Snapshot) -> Result<SchemaDiff> {
    let mut statements = Vec::new();

    // Diff extensions (PostgreSQL)
    diff_extensions(&from.extensions, &to.extensions, &mut statements);

    // Diff schemas
    diff_schemas(&from.schemas, &to.schemas, &mut statements);

    // Diff enums
    diff_enums(&from.enums, &to.enums, &mut statements);

    // Diff sequences
    diff_sequences(&from.sequences, &to.sequences, &mut statements);

    // Diff tables
    diff_tables(&from.tables, &to.tables, from.dialect, &mut statements);

    // Diff views
    diff_views(&from.views, &to.views, &mut statements);

    Ok(SchemaDiff { statements })
}

fn diff_extensions(from: &[String], to: &[String], statements: &mut Vec<DiffStatement>) {
    // Extensions to create
    for ext in to {
        if !from.contains(ext) {
            statements.push(DiffStatement::CreateExtension(ext.clone()));
        }
    }

    // Extensions to drop
    for ext in from {
        if !to.contains(ext) {
            statements.push(DiffStatement::DropExtension(ext.clone()));
        }
    }
}

fn diff_schemas(from: &[String], to: &[String], statements: &mut Vec<DiffStatement>) {
    // Schemas to create
    for schema in to {
        if !from.contains(schema) && schema != "public" && schema != "main" {
            statements.push(DiffStatement::CreateSchema {
                name: schema.clone(),
            });
        }
    }

    // Schemas to drop
    for schema in from {
        if !to.contains(schema) && schema != "public" && schema != "main" {
            statements.push(DiffStatement::DropSchema {
                name: schema.clone(),
                cascade: false,
            });
        }
    }
}

fn diff_enums(
    from: &IndexMap<String, EnumSnapshot>,
    to: &IndexMap<String, EnumSnapshot>,
    statements: &mut Vec<DiffStatement>,
) {
    // Enums to create
    for (name, enum_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::CreateEnum {
                name: enum_to.name.clone(),
                schema: enum_to.schema.clone(),
                values: enum_to.values.clone(),
                description: enum_to.description.clone(),
            });
        }
    }

    // Enums to drop
    for (name, enum_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropEnum {
                name: enum_from.name.clone(),
                schema: enum_from.schema.clone(),
                prev: enum_from.clone(),
            });
        }
    }

    // Enums to modify (add values)
    for (name, enum_to) in to {
        if let Some(enum_from) = from.get(name) {
            // Find new values
            let mut prev_value: Option<&String> = None;
            for value in &enum_to.values {
                if !enum_from.values.contains(value) {
                    let position = match prev_value {
                        Some(pv) => EnumValuePosition::After(pv.clone()),
                        None => EnumValuePosition::End,
                    };
                    statements.push(DiffStatement::AddEnumValue {
                        enum_name: enum_to.name.clone(),
                        schema: enum_to.schema.clone(),
                        value: value.clone(),
                        position,
                    });
                }
                prev_value = Some(value);
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
        }
    }
}

fn diff_sequences(
    from: &IndexMap<String, SequenceSnapshot>,
    to: &IndexMap<String, SequenceSnapshot>,
    statements: &mut Vec<DiffStatement>,
) {
    // Sequences to create
    for (name, seq_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::CreateSequence {
                sequence: seq_to.clone(),
            });
        }
    }

    // Sequences to drop
    for (name, seq_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropSequence {
                name: seq_from.name.clone(),
                schema: seq_from.schema.clone(),
                prev: seq_from.clone(),
            });
        }
    }

    // Sequences to alter
    for (name, seq_to) in to {
        if let Some(seq_from) = from.get(name) {
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
        }
    }
}

fn diff_tables(
    from: &IndexMap<String, TableSnapshot>,
    to: &IndexMap<String, TableSnapshot>,
    dialect: SchemaDialect,
    statements: &mut Vec<DiffStatement>,
) {
    // Tables to create
    for (name, table_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::CreateTable {
                table: table_to.clone(),
            });
        }
    }

    // Tables to drop
    for (name, table_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropTable {
                name: table_from.name.clone(),
                schema: table_from.schema.clone(),
                cascade: false,
                prev: table_from.clone(),
            });
        }
    }

    // Tables to alter
    for (name, table_to) in to {
        if let Some(table_from) = from.get(name) {
            diff_table(table_from, table_to, dialect, statements);
        }
    }
}

fn diff_table(
    from: &TableSnapshot,
    to: &TableSnapshot,
    _dialect: SchemaDialect,
    statements: &mut Vec<DiffStatement>,
) {
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

    // Diff columns
    diff_columns(&from.columns, &to.columns, &table, &schema, statements);

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

fn diff_columns(
    from: &IndexMap<String, ColumnSnapshot>,
    to: &IndexMap<String, ColumnSnapshot>,
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    // Columns to add
    for (name, col_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::AddColumn {
                table: table.to_string(),
                schema: schema.clone(),
                column: col_to.clone(),
            });
        }
    }

    // Columns to drop
    for (name, col_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropColumn {
                table: table.to_string(),
                schema: schema.clone(),
                column: name.clone(),
                cascade: false,
                prev: col_from.clone(),
            });
        }
    }

    // Columns to alter
    for (name, col_to) in to {
        if let Some(col_from) = from.get(name) {
            let changes = diff_column(col_from, col_to);
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
        }
    }
}

fn diff_column(from: &ColumnSnapshot, to: &ColumnSnapshot) -> Vec<ColumnChange> {
    let mut changes = Vec::new();

    // Type change
    if from.data_type != to.data_type {
        changes.push(ColumnChange::SetType(to.data_type.clone()));
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
        (Some(d1), Some(d2)) if d1 != d2 => changes.push(ColumnChange::SetDefault(d2.clone())),
        _ => {}
    }

    // Generated column change
    match (&from.generated, &to.generated) {
        (None, Some(g)) => changes.push(ColumnChange::SetGenerated(g.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropGenerated),
        (Some(g1), Some(g2)) if g1 != g2 => changes.push(ColumnChange::SetGenerated(g2.clone())),
        _ => {}
    }

    changes
}

fn diff_indexes(
    from: &IndexMap<String, IndexSnapshot>,
    to: &IndexMap<String, IndexSnapshot>,
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    // Indexes to create
    for (name, idx_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::CreateIndex {
                table: table.to_string(),
                schema: schema.clone(),
                index: idx_to.clone(),
                concurrently: false,
                if_not_exists: false,
            });
        }
    }

    // Indexes to drop
    for (name, idx_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropIndex {
                table: table.to_string(),
                name: name.clone(),
                schema: schema.clone(),
                concurrently: false,
                if_exists: false,
                prev: idx_from.clone(),
            });
        }
    }

    // Indexes to recreate (if changed)
    for (name, idx_to) in to {
        if let Some(idx_from) = from.get(name)
            && idx_from != idx_to
        {
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
    }
}

fn diff_constraints(
    from: &[ConstraintSnapshot],
    to: &[ConstraintSnapshot],
    table: &str,
    schema: &Option<String>,
    statements: &mut Vec<DiffStatement>,
) {
    // Build maps by name for named constraints
    let from_by_name: IndexMap<String, &ConstraintSnapshot> = from
        .iter()
        .filter_map(|c| c.name.as_ref().map(|n| (n.clone(), c)))
        .collect();

    let to_by_name: IndexMap<String, &ConstraintSnapshot> = to
        .iter()
        .filter_map(|c| c.name.as_ref().map(|n| (n.clone(), c)))
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
            && constraint_from != constraint_to
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

fn diff_views(
    from: &IndexMap<String, ViewSnapshot>,
    to: &IndexMap<String, ViewSnapshot>,
    statements: &mut Vec<DiffStatement>,
) {
    // Views to create
    for (name, view_to) in to {
        if !from.contains_key(name) {
            statements.push(DiffStatement::CreateView {
                view: view_to.clone(),
                or_replace: false,
            });
        }
    }

    // Views to drop
    for (name, view_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropView {
                name: view_from.name.clone(),
                schema: view_from.schema.clone(),
                materialized: view_from.materialized,
                cascade: false,
                prev: view_from.clone(),
            });
        }
    }

    // Views to alter
    for (name, view_to) in to {
        if let Some(view_from) = from.get(name)
            && view_from.definition != view_to.definition
        {
            statements.push(DiffStatement::AlterView {
                name: view_to.name.clone(),
                schema: view_to.schema.clone(),
                new_definition: view_to.definition.clone(),
                prev_definition: view_from.definition.clone(),
            });
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
                DiffStatement::CreateSchema { .. } => summary.schemas_created += 1,
                DiffStatement::DropSchema { .. } => summary.schemas_dropped += 1,
                DiffStatement::CreateEnum { .. } => summary.enums_created += 1,
                DiffStatement::DropEnum { .. } => summary.enums_dropped += 1,
                DiffStatement::AddEnumValue { .. } => summary.enum_values_added += 1,
                DiffStatement::AlterEnumDescription { .. } => summary.enums_altered += 1,
                DiffStatement::CreateSequence { .. } => summary.sequences_created += 1,
                DiffStatement::DropSequence { .. } => summary.sequences_dropped += 1,
                DiffStatement::AlterSequence { .. } => summary.sequences_altered += 1,
                DiffStatement::CreateTable { .. } => summary.tables_created += 1,
                DiffStatement::DropTable { .. } => summary.tables_dropped += 1,
                DiffStatement::RenameTable { .. } => summary.tables_renamed += 1,
                DiffStatement::AlterTableComment { .. } => summary.tables_altered += 1,
                DiffStatement::AddColumn { .. } => summary.columns_added += 1,
                DiffStatement::DropColumn { .. } => summary.columns_dropped += 1,
                DiffStatement::RenameColumn { .. } => summary.columns_renamed += 1,
                DiffStatement::AlterColumn { .. } => summary.columns_altered += 1,
                DiffStatement::AlterColumnComment { .. } => summary.columns_altered += 1,
                DiffStatement::CreateIndex { .. } => summary.indexes_created += 1,
                DiffStatement::DropIndex { .. } => summary.indexes_dropped += 1,
                DiffStatement::AddConstraint { .. } => summary.constraints_added += 1,
                DiffStatement::DropConstraint { .. } => summary.constraints_dropped += 1,
                DiffStatement::CreateView { .. } => summary.views_created += 1,
                DiffStatement::DropView { .. } => summary.views_dropped += 1,
                DiffStatement::AlterView { .. } => summary.views_altered += 1,
                DiffStatement::CreateExtension(_) => summary.extensions_created += 1,
                DiffStatement::DropExtension(_) => summary.extensions_dropped += 1,
                DiffStatement::RenameSchema { .. } => summary.schemas_renamed += 1,
                DiffStatement::RenameEnum { .. } => summary.enums_renamed += 1,
            }
        }

        summary
    }
}

/// Summary of changes in a diff
#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    pub schemas_created: usize,
    pub schemas_dropped: usize,
    pub enums_created: usize,
    pub enums_dropped: usize,
    pub enums_altered: usize,
    pub enum_values_added: usize,
    pub enums_renamed: usize,
    pub sequences_created: usize,
    pub sequences_dropped: usize,
    pub sequences_altered: usize,
    pub tables_created: usize,
    pub tables_dropped: usize,
    pub tables_renamed: usize,
    pub tables_altered: usize,
    pub columns_added: usize,
    pub columns_dropped: usize,
    pub columns_renamed: usize,
    pub columns_altered: usize,
    pub indexes_created: usize,
    pub indexes_dropped: usize,
    pub constraints_added: usize,
    pub constraints_dropped: usize,
    pub views_created: usize,
    pub views_dropped: usize,
    pub views_altered: usize,
    pub extensions_created: usize,
    pub extensions_dropped: usize,
    pub schemas_renamed: usize,
}

impl std::fmt::Display for DiffSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();

        if self.tables_created > 0 {
            parts.push(format!("- {} table(s) created", self.tables_created));
        }
        if self.tables_dropped > 0 {
            parts.push(format!("- {} table(s) dropped", self.tables_dropped));
        }
        if self.tables_altered > 0 {
            parts.push(format!("- {} table(s) altered", self.tables_altered));
        }
        if self.columns_added > 0 {
            parts.push(format!("- {} column(s) added", self.columns_added));
        }
        if self.columns_dropped > 0 {
            parts.push(format!("- {} column(s) dropped", self.columns_dropped));
        }
        if self.columns_altered > 0 {
            parts.push(format!("- {} column(s) altered", self.columns_altered));
        }
        if self.indexes_created > 0 {
            parts.push(format!("- {} index(es) created", self.indexes_created));
        }
        if self.indexes_dropped > 0 {
            parts.push(format!("- {} index(es) dropped", self.indexes_dropped));
        }
        if self.constraints_added > 0 {
            parts.push(format!("- {} constraint(s) added", self.constraints_added));
        }
        if self.constraints_dropped > 0 {
            parts.push(format!(
                "- {} constraint(s) dropped",
                self.constraints_dropped
            ));
        }
        if self.enums_created > 0 {
            parts.push(format!("- {} enum(s) created", self.enums_created));
        }
        if self.enums_dropped > 0 {
            parts.push(format!("- {} enum(s) dropped", self.enums_dropped));
        }
        if self.enums_altered > 0 {
            parts.push(format!("- {} enum(s) altered", self.enums_altered));
        }
        if self.views_created > 0 {
            parts.push(format!("- {} view(s) created", self.views_created));
        }
        if self.views_dropped > 0 {
            parts.push(format!("- {} view(s) dropped", self.views_dropped));
        }

        if parts.is_empty() {
            write!(f, "No changes")
        } else {
            write!(f, "{}", parts.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rename::{apply_renames, RenameDecision, RenameDecisions};
    use crate::schema::SchemaDialect;
    use crate::snapshot::{ConstraintType, ForeignKeyReference};

    fn empty_snapshot() -> Snapshot {
        Snapshot::new(SchemaDialect::Postgres)
    }

    #[test]
    fn test_diff_enum_description_added() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: Some("Status of the entity".to_string()),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterEnumDescription {
                name, description, ..
            } => {
                assert_eq!(name, "status");
                assert_eq!(*description, Some("Status of the entity".to_string()));
            }
            _ => panic!("Expected AlterEnumDescription"),
        }
    }

    #[test]
    fn test_diff_enum_description_changed() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("Old description".to_string()),
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("New description".to_string()),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterEnumDescription {
                name, description, ..
            } => {
                assert_eq!(name, "status");
                assert_eq!(*description, Some("New description".to_string()));
            }
            _ => panic!("Expected AlterEnumDescription"),
        }
    }

    #[test]
    fn test_diff_enum_description_removed() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("Some description".to_string()),
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: None,
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterEnumDescription {
                name, description, ..
            } => {
                assert_eq!(name, "status");
                assert_eq!(*description, None);
            }
            _ => panic!("Expected AlterEnumDescription"),
        }
    }

    #[test]
    fn test_diff_enum_description_unchanged() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("Same description".to_string()),
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("Same description".to_string()),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_diff_enum_values_and_description_changed() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: Some("Old description".to_string()),
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "pending".to_string()],
                description: Some("New description".to_string()),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 2);

        let has_add_value = diff
            .statements
            .iter()
            .any(|s| matches!(s, DiffStatement::AddEnumValue { value, .. } if value == "pending"));
        let has_alter_desc = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterEnumDescription { description, .. } if *description == Some("New description".to_string()))
        });

        assert!(has_add_value, "Expected AddEnumValue statement");
        assert!(has_alter_desc, "Expected AlterEnumDescription statement");
    }

    fn create_table_snapshot(name: &str, comment: Option<&str>) -> TableSnapshot {
        use crate::snapshot::ColumnSnapshot;
        use indexmap::IndexMap;

        let mut columns = IndexMap::new();
        columns.insert(
            "id".to_string(),
            ColumnSnapshot {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                nullable: false,
                default: None,
                primary_key: true,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );

        TableSnapshot {
            name: name.to_string(),
            schema: None,
            columns,
            constraints: vec![],
            indexes: IndexMap::new(),
            comment: comment.map(|s| s.to_string()),
        }
    }

    fn create_column_snapshot(name: &str, comment: Option<&str>) -> ColumnSnapshot {
        ColumnSnapshot {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default: None,
            primary_key: false,
            unique: false,
            generated: None,
            identity: None,
            comment: comment.map(|s| s.to_string()),
            collation: None,
        }
    }

    #[test]
    fn test_diff_table_comment_added() {
        let mut from = empty_snapshot();
        from.tables
            .insert("users".to_string(), create_table_snapshot("users", None));

        let mut to = empty_snapshot();
        to.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("User accounts table")),
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterTableComment { table, comment, .. } => {
                assert_eq!(table, "users");
                assert_eq!(*comment, Some("User accounts table".to_string()));
            }
            _ => panic!("Expected AlterTableComment"),
        }
    }

    #[test]
    fn test_diff_table_comment_changed() {
        let mut from = empty_snapshot();
        from.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("Old comment")),
        );

        let mut to = empty_snapshot();
        to.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("New comment")),
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterTableComment { table, comment, .. } => {
                assert_eq!(table, "users");
                assert_eq!(*comment, Some("New comment".to_string()));
            }
            _ => panic!("Expected AlterTableComment"),
        }
    }

    #[test]
    fn test_diff_table_comment_removed() {
        let mut from = empty_snapshot();
        from.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("Some comment")),
        );

        let mut to = empty_snapshot();
        to.tables
            .insert("users".to_string(), create_table_snapshot("users", None));

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterTableComment { table, comment, .. } => {
                assert_eq!(table, "users");
                assert_eq!(*comment, None);
            }
            _ => panic!("Expected AlterTableComment"),
        }
    }

    #[test]
    fn test_diff_table_comment_unchanged() {
        let mut from = empty_snapshot();
        from.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("Same comment")),
        );

        let mut to = empty_snapshot();
        to.tables.insert(
            "users".to_string(),
            create_table_snapshot("users", Some("Same comment")),
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_diff_column_comment_added() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from
            .columns
            .insert("email".to_string(), create_column_snapshot("email", None));
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("User email address")),
        );
        to.tables.insert("users".to_string(), table_to);

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterColumnComment {
                table,
                column,
                comment,
                ..
            } => {
                assert_eq!(table, "users");
                assert_eq!(column, "email");
                assert_eq!(comment, &Some("User email address".to_string()));
            }
            _ => panic!("Expected AlterColumnComment"),
        }
    }

    #[test]
    fn test_diff_column_comment_changed() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("Old comment")),
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("New comment")),
        );
        to.tables.insert("users".to_string(), table_to);

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterColumnComment {
                table,
                column,
                comment,
                ..
            } => {
                assert_eq!(table, "users");
                assert_eq!(column, "email");
                assert_eq!(comment, &Some("New comment".to_string()));
            }
            _ => panic!("Expected AlterColumnComment"),
        }
    }

    #[test]
    fn test_diff_column_comment_removed() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("Some comment")),
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to
            .columns
            .insert("email".to_string(), create_column_snapshot("email", None));
        to.tables.insert("users".to_string(), table_to);

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 1);
        match &diff.statements[0] {
            DiffStatement::AlterColumnComment {
                table,
                column,
                comment,
                ..
            } => {
                assert_eq!(table, "users");
                assert_eq!(column, "email");
                assert_eq!(comment, &None);
            }
            _ => panic!("Expected AlterColumnComment"),
        }
    }

    #[test]
    fn test_diff_column_comment_unchanged() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("Same comment")),
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("Same comment")),
        );
        to.tables.insert("users".to_string(), table_to);

        let diff = diff_snapshots(&from, &to).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_diff_table_and_column_comments_changed() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", Some("Old table comment"));
        table_from.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("Old column comment")),
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", Some("New table comment"));
        table_to.columns.insert(
            "email".to_string(),
            create_column_snapshot("email", Some("New column comment")),
        );
        to.tables.insert("users".to_string(), table_to);

        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 2);

        let has_table_comment = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterTableComment { comment, .. } if comment == &Some("New table comment".to_string()))
        });
        let has_column_comment = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumnComment { comment, .. } if comment == &Some("New column comment".to_string()))
        });

        assert!(has_table_comment, "Expected AlterTableComment statement");
        assert!(has_column_comment, "Expected AlterColumnComment statement");
    }

    // ==================== DiffStatement::undo() Tests ====================

    #[test]
    fn test_undo_create_schema() {
        let stmt = DiffStatement::CreateSchema {
            name: "myschema".to_string(),
        };
        let undone = stmt
            .undo_statement()
            .expect("CreateSchema should be reversible");
        match undone {
            DiffStatement::DropSchema { name, cascade } => {
                assert_eq!(name, "myschema");
                assert!(!cascade);
            }
            _ => panic!("Expected DropSchema"),
        }
    }

    #[test]
    fn test_undo_drop_schema() {
        let stmt = DiffStatement::DropSchema {
            name: "myschema".to_string(),
            cascade: true,
        };
        let undone = stmt
            .undo_statement()
            .expect("DropSchema should be reversible");
        match undone {
            DiffStatement::CreateSchema { name } => {
                assert_eq!(name, "myschema");
            }
            _ => panic!("Expected CreateSchema"),
        }
    }

    #[test]
    fn test_undo_rename_schema() {
        let stmt = DiffStatement::RenameSchema {
            from: "old_name".to_string(),
            to: "new_name".to_string(),
        };
        let undone = stmt
            .undo_statement()
            .expect("RenameSchema should be reversible");
        match undone {
            DiffStatement::RenameSchema { from, to } => {
                assert_eq!(from, "new_name");
                assert_eq!(to, "old_name");
            }
            _ => panic!("Expected RenameSchema"),
        }
    }

    #[test]
    fn test_undo_create_enum() {
        let stmt = DiffStatement::CreateEnum {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            values: vec!["active".to_string(), "inactive".to_string()],
            description: Some("Status enum".to_string()),
        };
        let undone = stmt
            .undo_statement()
            .expect("CreateEnum should be reversible");
        match undone {
            DiffStatement::DropEnum { name, schema, .. } => {
                assert_eq!(name, "status");
                assert_eq!(schema, Some("public".to_string()));
            }
            _ => panic!("Expected DropEnum"),
        }
    }

    #[test]
    fn test_undo_drop_enum_is_reversible() {
        let stmt = DiffStatement::DropEnum {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            prev: EnumSnapshot {
                name: "status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["active".to_string(), "inactive".to_string()],
                description: Some("Status enum".to_string()),
            },
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropEnum should be reversible");
        match undo.unwrap() {
            DiffStatement::CreateEnum {
                name,
                schema,
                values,
                description,
            } => {
                assert_eq!(name, "status");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(values, vec!["active", "inactive"]);
                assert_eq!(description, Some("Status enum".to_string()));
            }
            _ => panic!("Expected CreateEnum"),
        }
    }

    #[test]
    fn test_undo_add_enum_value_not_reversible() {
        let stmt = DiffStatement::AddEnumValue {
            enum_name: "status".to_string(),
            schema: None,
            value: "pending".to_string(),
            position: EnumValuePosition::End,
        };
        assert!(
            stmt.undo_statement().is_none(),
            "AddEnumValue should not be reversible"
        );
    }

    #[test]
    fn test_undo_rename_enum() {
        let stmt = DiffStatement::RenameEnum {
            from: "old_enum".to_string(),
            to: "new_enum".to_string(),
            schema: Some("public".to_string()),
        };
        let undone = stmt
            .undo_statement()
            .expect("RenameEnum should be reversible");
        match undone {
            DiffStatement::RenameEnum { from, to, schema } => {
                assert_eq!(from, "new_enum");
                assert_eq!(to, "old_enum");
                assert_eq!(schema, Some("public".to_string()));
            }
            _ => panic!("Expected RenameEnum"),
        }
    }

    #[test]
    fn test_undo_create_table() {
        let table = TableSnapshot {
            name: "users".to_string(),
            schema: Some("public".to_string()),
            columns: IndexMap::new(),
            constraints: vec![],
            indexes: IndexMap::new(),
            comment: None,
        };
        let stmt = DiffStatement::CreateTable { table };
        let undone = stmt
            .undo_statement()
            .expect("CreateTable should be reversible");
        match undone {
            DiffStatement::DropTable {
                name,
                schema,
                cascade,
                ..
            } => {
                assert_eq!(name, "users");
                assert_eq!(schema, Some("public".to_string()));
                assert!(!cascade);
            }
            _ => panic!("Expected DropTable"),
        }
    }

    #[test]
    fn test_undo_drop_table_is_reversible() {
        let table = TableSnapshot {
            name: "users".to_string(),
            schema: Some("public".to_string()),
            columns: IndexMap::new(),
            constraints: vec![],
            indexes: IndexMap::new(),
            comment: None,
        };
        let stmt = DiffStatement::DropTable {
            name: "users".to_string(),
            schema: Some("public".to_string()),
            cascade: false,
            prev: table,
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropTable should be reversible");
        match undo.unwrap() {
            DiffStatement::CreateTable { table } => {
                assert_eq!(table.name, "users");
                assert_eq!(table.schema, Some("public".to_string()));
            }
            _ => panic!("Expected CreateTable"),
        }
    }

    #[test]
    fn test_undo_rename_table() {
        let stmt = DiffStatement::RenameTable {
            from: "old_table".to_string(),
            to: "new_table".to_string(),
            schema: None,
        };
        let undone = stmt
            .undo_statement()
            .expect("RenameTable should be reversible");
        match undone {
            DiffStatement::RenameTable { from, to, schema } => {
                assert_eq!(from, "new_table");
                assert_eq!(to, "old_table");
                assert!(schema.is_none());
            }
            _ => panic!("Expected RenameTable"),
        }
    }

    #[test]
    fn test_undo_add_column() {
        let column = ColumnSnapshot {
            name: "email".to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default: None,
            primary_key: false,
            unique: false,
            generated: None,
            identity: None,
            comment: None,
            collation: None,
        };
        let stmt = DiffStatement::AddColumn {
            table: "users".to_string(),
            schema: None,
            column,
        };
        let undone = stmt
            .undo_statement()
            .expect("AddColumn should be reversible");
        match undone {
            DiffStatement::DropColumn {
                table,
                schema,
                column,
                cascade,
                ..
            } => {
                assert_eq!(table, "users");
                assert!(schema.is_none());
                assert_eq!(column, "email");
                assert!(!cascade);
            }
            _ => panic!("Expected DropColumn"),
        }
    }

    #[test]
    fn test_undo_drop_column_is_reversible() {
        let prev = ColumnSnapshot {
            name: "email".to_string(),
            data_type: "text".to_string(),
            nullable: false,
            default: None,
            primary_key: false,
            unique: true,
            generated: None,
            identity: None,
            comment: None,
            collation: None,
        };
        let stmt = DiffStatement::DropColumn {
            table: "users".to_string(),
            schema: Some("public".to_string()),
            column: "email".to_string(),
            cascade: false,
            prev,
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropColumn should be reversible");
        match undo.unwrap() {
            DiffStatement::AddColumn {
                table,
                schema,
                column,
            } => {
                assert_eq!(table, "users");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(column.name, "email");
                assert_eq!(column.data_type, "text");
            }
            _ => panic!("Expected AddColumn"),
        }
    }

    #[test]
    fn test_undo_rename_column() {
        let stmt = DiffStatement::RenameColumn {
            table: "users".to_string(),
            schema: None,
            from: "old_col".to_string(),
            to: "new_col".to_string(),
        };
        let undone = stmt
            .undo_statement()
            .expect("RenameColumn should be reversible");
        match undone {
            DiffStatement::RenameColumn {
                table,
                schema,
                from,
                to,
            } => {
                assert_eq!(table, "users");
                assert!(schema.is_none());
                assert_eq!(from, "new_col");
                assert_eq!(to, "old_col");
            }
            _ => panic!("Expected RenameColumn"),
        }
    }

    #[test]
    fn test_undo_create_index() {
        let index = IndexSnapshot {
            name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
            method: "btree".to_string(),
            where_clause: None,
            include: vec![],
        };
        let stmt = DiffStatement::CreateIndex {
            table: "users".to_string(),
            schema: Some("public".to_string()),
            index,
            concurrently: false,
            if_not_exists: false,
        };
        let undone = stmt
            .undo_statement()
            .expect("CreateIndex should be reversible");
        match undone {
            DiffStatement::DropIndex {
                name,
                schema,
                concurrently,
                if_exists,
                ..
            } => {
                assert_eq!(name, "idx_users_email");
                assert_eq!(schema, Some("public".to_string()));
                assert!(!concurrently);
                assert!(!if_exists);
            }
            _ => panic!("Expected DropIndex"),
        }
    }

    #[test]
    fn test_undo_drop_index_is_reversible() {
        let prev = IndexSnapshot {
            name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
            method: "btree".to_string(),
            where_clause: None,
            include: vec![],
        };
        let stmt = DiffStatement::DropIndex {
            table: "users".to_string(),
            name: "idx_users_email".to_string(),
            schema: Some("public".to_string()),
            concurrently: false,
            if_exists: false,
            prev,
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropIndex should be reversible");
        match undo.unwrap() {
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
                ..
            } => {
                assert_eq!(table, "users");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(index.name, "idx_users_email");
                assert_eq!(index.columns, vec!["email"]);
            }
            _ => panic!("Expected CreateIndex"),
        }
    }

    #[test]
    fn test_undo_add_constraint_named() {
        let constraint = ConstraintSnapshot {
            name: Some("fk_posts_user".to_string()),
            constraint_type: ConstraintType::ForeignKey,
            columns: vec!["user_id".to_string()],
            references: None,
            expression: None,
        };
        let stmt = DiffStatement::AddConstraint {
            table: "posts".to_string(),
            schema: None,
            constraint,
        };
        let undone = stmt
            .undo_statement()
            .expect("AddConstraint with name should be reversible");
        match undone {
            DiffStatement::DropConstraint {
                table,
                schema,
                name,
                cascade,
                ..
            } => {
                assert_eq!(table, "posts");
                assert!(schema.is_none());
                assert_eq!(name, "fk_posts_user");
                assert!(!cascade);
            }
            _ => panic!("Expected DropConstraint"),
        }
    }

    #[test]
    fn test_undo_add_constraint_unnamed_not_reversible() {
        let constraint = ConstraintSnapshot {
            name: None,
            constraint_type: ConstraintType::Check,
            columns: vec![],
            references: None,
            expression: Some("age > 0".to_string()),
        };
        let stmt = DiffStatement::AddConstraint {
            table: "users".to_string(),
            schema: None,
            constraint,
        };
        assert!(
            stmt.undo_statement().is_none(),
            "AddConstraint without name should not be reversible"
        );
    }

    #[test]
    fn test_undo_create_view() {
        let view = ViewSnapshot {
            name: "active_users".to_string(),
            schema: None,
            definition: "SELECT * FROM users WHERE active = true".to_string(),
            materialized: false,
        };
        let stmt = DiffStatement::CreateView {
            view,
            or_replace: false,
        };
        let undone = stmt
            .undo_statement()
            .expect("CreateView should be reversible");
        match undone {
            DiffStatement::DropView {
                name,
                schema,
                materialized,
                cascade,
                ..
            } => {
                assert_eq!(name, "active_users");
                assert!(schema.is_none());
                assert!(!materialized);
                assert!(!cascade);
            }
            _ => panic!("Expected DropView"),
        }
    }

    #[test]
    fn test_undo_create_extension() {
        let stmt = DiffStatement::CreateExtension("uuid-ossp".to_string());
        let undone = stmt
            .undo_statement()
            .expect("CreateExtension should be reversible");
        match undone {
            DiffStatement::DropExtension(name) => {
                assert_eq!(name, "uuid-ossp");
            }
            _ => panic!("Expected DropExtension"),
        }
    }

    #[test]
    fn test_undo_drop_extension() {
        let stmt = DiffStatement::DropExtension("uuid-ossp".to_string());
        let undone = stmt
            .undo_statement()
            .expect("DropExtension should be reversible");
        match undone {
            DiffStatement::CreateExtension(name) => {
                assert_eq!(name, "uuid-ossp");
            }
            _ => panic!("Expected CreateExtension"),
        }
    }

    #[test]
    fn test_undo_create_sequence() {
        let sequence = SequenceSnapshot {
            name: "user_id_seq".to_string(),
            schema: Some("public".to_string()),
            increment: 1,
            min_value: 1,
            max_value: None,
            start: 1,
            cache: 1,
            cycle: false,
        };
        let stmt = DiffStatement::CreateSequence { sequence };
        let undone = stmt
            .undo_statement()
            .expect("CreateSequence should be reversible");
        match undone {
            DiffStatement::DropSequence { name, schema, .. } => {
                assert_eq!(name, "user_id_seq");
                assert_eq!(schema, Some("public".to_string()));
            }
            _ => panic!("Expected DropSequence"),
        }
    }

    #[test]
    fn test_undo_drop_sequence_is_reversible() {
        let prev = SequenceSnapshot {
            name: "user_id_seq".to_string(),
            schema: Some("public".to_string()),
            increment: 1,
            min_value: 1,
            max_value: Some(1000000),
            start: 100,
            cache: 10,
            cycle: true,
        };
        let stmt = DiffStatement::DropSequence {
            name: "user_id_seq".to_string(),
            schema: Some("public".to_string()),
            prev: prev.clone(),
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropSequence should be reversible");
        match undo.unwrap() {
            DiffStatement::CreateSequence { sequence } => {
                assert_eq!(sequence.name, "user_id_seq");
                assert_eq!(sequence.schema, Some("public".to_string()));
                assert_eq!(sequence.increment, 1);
                assert_eq!(sequence.max_value, Some(1000000));
                assert_eq!(sequence.start, 100);
                assert_eq!(sequence.cache, 10);
                assert!(sequence.cycle);
            }
            _ => panic!("Expected CreateSequence"),
        }
    }

    #[test]
    fn test_undo_drop_constraint_is_reversible() {
        let prev = ConstraintSnapshot {
            name: Some("fk_user_id".to_string()),
            constraint_type: ConstraintType::ForeignKey,
            columns: vec!["user_id".to_string()],
            references: Some(ForeignKeyReference {
                schema: None,
                table: "users".to_string(),
                columns: vec!["id".to_string()],
                on_delete: "CASCADE".to_string(),
                on_update: "NO ACTION".to_string(),
            }),
            expression: None,
        };
        let stmt = DiffStatement::DropConstraint {
            table: "posts".to_string(),
            schema: Some("public".to_string()),
            name: "fk_user_id".to_string(),
            cascade: false,
            prev,
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropConstraint should be reversible");
        match undo.unwrap() {
            DiffStatement::AddConstraint {
                table,
                schema,
                constraint,
            } => {
                assert_eq!(table, "posts");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(constraint.name, Some("fk_user_id".to_string()));
                assert!(matches!(
                    constraint.constraint_type,
                    ConstraintType::ForeignKey
                ));
            }
            _ => panic!("Expected AddConstraint"),
        }
    }

    #[test]
    fn test_undo_drop_view_is_reversible() {
        let prev = ViewSnapshot {
            name: "active_users".to_string(),
            schema: Some("public".to_string()),
            definition: "SELECT * FROM users WHERE active = true".to_string(),
            materialized: false,
        };
        let stmt = DiffStatement::DropView {
            name: "active_users".to_string(),
            schema: Some("public".to_string()),
            materialized: false,
            cascade: false,
            prev,
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "DropView should be reversible");
        match undo.unwrap() {
            DiffStatement::CreateView { view, or_replace } => {
                assert_eq!(view.name, "active_users");
                assert_eq!(view.schema, Some("public".to_string()));
                assert_eq!(view.definition, "SELECT * FROM users WHERE active = true");
                assert!(!view.materialized);
                assert!(!or_replace);
            }
            _ => panic!("Expected CreateView"),
        }
    }

    #[test]
    fn test_undo_alter_enum_description_is_reversible() {
        let stmt = DiffStatement::AlterEnumDescription {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            description: Some("New description".to_string()),
            prev_description: Some("Old description".to_string()),
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "AlterEnumDescription should be reversible");
        match undo.unwrap() {
            DiffStatement::AlterEnumDescription {
                name,
                schema,
                description,
                prev_description,
            } => {
                assert_eq!(name, "status");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(description, Some("Old description".to_string()));
                assert_eq!(prev_description, Some("New description".to_string()));
            }
            _ => panic!("Expected AlterEnumDescription"),
        }
    }

    #[test]
    fn test_undo_alter_column_comment_is_reversible() {
        let stmt = DiffStatement::AlterColumnComment {
            table: "users".to_string(),
            schema: Some("public".to_string()),
            column: "email".to_string(),
            comment: Some("New comment".to_string()),
            prev_comment: Some("Old comment".to_string()),
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "AlterColumnComment should be reversible");
        match undo.unwrap() {
            DiffStatement::AlterColumnComment {
                table,
                schema,
                column,
                comment,
                prev_comment,
            } => {
                assert_eq!(table, "users");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(column, "email");
                assert_eq!(comment, Some("Old comment".to_string()));
                assert_eq!(prev_comment, Some("New comment".to_string()));
            }
            _ => panic!("Expected AlterColumnComment"),
        }
    }

    #[test]
    fn test_undo_alter_view_is_reversible() {
        let stmt = DiffStatement::AlterView {
            name: "active_users".to_string(),
            schema: Some("public".to_string()),
            new_definition: "SELECT * FROM users WHERE active = true AND verified = true"
                .to_string(),
            prev_definition: "SELECT * FROM users WHERE active = true".to_string(),
        };
        let undo = stmt.undo_statement();
        assert!(undo.is_some(), "AlterView should be reversible");
        match undo.unwrap() {
            DiffStatement::AlterView {
                name,
                schema,
                new_definition,
                prev_definition,
            } => {
                assert_eq!(name, "active_users");
                assert_eq!(schema, Some("public".to_string()));
                assert_eq!(new_definition, "SELECT * FROM users WHERE active = true");
                assert_eq!(
                    prev_definition,
                    "SELECT * FROM users WHERE active = true AND verified = true"
                );
            }
            _ => panic!("Expected AlterView"),
        }
    }

    // ==================== SchemaDiff::undo() Tests ====================

    #[test]
    fn test_schema_diff_undo_reversible() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "myschema".to_string(),
                },
                DiffStatement::CreateExtension("uuid-ossp".to_string()),
            ],
        };

        let (reversed, irreversible) = diff.get_down_diff();

        assert!(irreversible.is_empty());
        assert_eq!(reversed.statements.len(), 2);

        // Statements should be in reverse order
        match &reversed.statements[0] {
            DiffStatement::DropExtension(name) => assert_eq!(name, "uuid-ossp"),
            _ => panic!("Expected DropExtension first"),
        }
        match &reversed.statements[1] {
            DiffStatement::DropSchema { name, .. } => assert_eq!(name, "myschema"),
            _ => panic!("Expected DropSchema second"),
        }
    }

    #[test]
    fn test_schema_diff_undo_mixed() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "myschema".to_string(),
                },
                DiffStatement::AddEnumValue {
                    enum_name: "status".to_string(),
                    schema: None,
                    value: "pending".to_string(),
                    position: EnumValuePosition::End,
                },
                DiffStatement::CreateExtension("uuid-ossp".to_string()),
            ],
        };

        let (reversed, irreversible) = diff.get_down_diff();

        // AddEnumValue is not reversible
        assert_eq!(irreversible.len(), 1);
        assert_eq!(reversed.statements.len(), 2);
    }

    #[test]
    fn test_schema_diff_is_fully_reversible() {
        let reversible_diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "myschema".to_string(),
                },
                DiffStatement::CreateExtension("uuid-ossp".to_string()),
            ],
        };
        assert!(reversible_diff.is_fully_reversible());

        let mixed_diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "myschema".to_string(),
                },
                DiffStatement::AddEnumValue {
                    enum_name: "status".to_string(),
                    schema: None,
                    value: "pending".to_string(),
                    position: EnumValuePosition::End,
                },
            ],
        };
        assert!(!mixed_diff.is_fully_reversible());
    }

    #[test]
    fn test_schema_diff_reversibility_stats() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::CreateSchema {
                    name: "myschema".to_string(),
                },
                DiffStatement::AddEnumValue {
                    enum_name: "status".to_string(),
                    schema: None,
                    value: "pending".to_string(),
                    position: EnumValuePosition::End,
                },
                DiffStatement::CreateExtension("uuid-ossp".to_string()),
                DiffStatement::AlterColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: "name".to_string(),
                    changes: vec![ColumnChange::SetNotNull],
                },
            ],
        };

        let (reversible, irreversible) = diff.reversibility_stats();
        assert_eq!(reversible, 2); // CreateSchema, CreateExtension
        assert_eq!(irreversible, 2); // AddEnumValue, AlterColumn
    }

    #[test]
    fn test_schema_diff_undo_empty() {
        let diff = SchemaDiff { statements: vec![] };
        let (reversed, irreversible) = diff.get_down_diff();

        assert!(reversed.is_empty());
        assert!(irreversible.is_empty());
    }

    #[test]
    fn test_diff_statement_is_reversible() {
        // Reversible statements
        assert!(
            DiffStatement::CreateSchema {
                name: "s".to_string()
            }
            .is_reversible()
        );
        assert!(DiffStatement::CreateExtension("e".to_string()).is_reversible());
        assert!(DiffStatement::DropExtension("e".to_string()).is_reversible());

        // Now-reversible statements (with prev field)
        assert!(
            DiffStatement::DropTable {
                name: "t".to_string(),
                schema: None,
                cascade: false,
                prev: TableSnapshot {
                    name: "t".to_string(),
                    schema: None,
                    columns: IndexMap::new(),
                    constraints: vec![],
                    indexes: IndexMap::new(),
                    comment: None,
                },
            }
            .is_reversible()
        );

        // Irreversible statements (PostgreSQL limitation)
        assert!(
            !DiffStatement::AddEnumValue {
                enum_name: "e".to_string(),
                schema: None,
                value: "v".to_string(),
                position: EnumValuePosition::End,
            }
            .is_reversible()
        );
    }

    // Tests for rename detection integration

    #[test]
    fn test_diff_with_column_rename() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "old_name".to_string(),
            create_column_snapshot("old_name", None),
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "new_name".to_string(),
            create_column_snapshot("new_name", None),
        );
        to.tables.insert("users".to_string(), table_to);

        // Without rename decision: should drop old and add new
        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 2);
        let has_drop = diff.statements.iter().any(|s| matches!(s, DiffStatement::DropColumn { column, .. } if column == "old_name"));
        let has_add = diff.statements.iter().any(|s| matches!(s, DiffStatement::AddColumn { column, .. } if column.name == "new_name"));
        assert!(has_drop, "Expected DropColumn for old_name");
        assert!(has_add, "Expected AddColumn for new_name");

        // With rename decision: should rename instead
        let mut renames = RenameDecisions::new();
        renames.columns.insert(
            ("users".to_string(), "old_name".to_string()),
            RenameDecision::Rename {
                from: "old_name".to_string(),
                to: "new_name".to_string(),
            },
        );

        let diff_with_rename = apply_renames(&diff, &renames);
        assert_eq!(diff_with_rename.statements.len(), 1);
        match &diff_with_rename.statements[0] {
            DiffStatement::RenameColumn { table, from, to, .. } => {
                assert_eq!(table, "users");
                assert_eq!(from, "old_name");
                assert_eq!(to, "new_name");
            }
            _ => panic!("Expected RenameColumn, got {:?}", diff_with_rename.statements[0]),
        }
    }

    #[test]
    fn test_diff_with_table_rename() {
        let mut from = empty_snapshot();
        from.tables.insert(
            "old_table".to_string(),
            create_table_snapshot("old_table", None),
        );

        let mut to = empty_snapshot();
        to.tables.insert(
            "new_table".to_string(),
            create_table_snapshot("new_table", None),
        );

        // Without rename decision: should drop old and create new
        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 2);
        let has_drop = diff.statements.iter().any(|s| matches!(s, DiffStatement::DropTable { name, .. } if name == "old_table"));
        let has_create = diff.statements.iter().any(|s| matches!(s, DiffStatement::CreateTable { table, .. } if table.name == "new_table"));
        assert!(has_drop, "Expected DropTable for old_table");
        assert!(has_create, "Expected CreateTable for new_table");

        // With rename decision: should rename instead
        let mut renames = RenameDecisions::new();
        renames.tables.insert(
            "old_table".to_string(),
            RenameDecision::Rename {
                from: "old_table".to_string(),
                to: "new_table".to_string(),
            },
        );

        let diff_with_rename = apply_renames(&diff, &renames);
        assert_eq!(diff_with_rename.statements.len(), 1);
        match &diff_with_rename.statements[0] {
            DiffStatement::RenameTable { from, to, .. } => {
                assert_eq!(from, "old_table");
                assert_eq!(to, "new_table");
            }
            _ => panic!("Expected RenameTable, got {:?}", diff_with_rename.statements[0]),
        }
    }

    #[test]
    fn test_diff_with_enum_rename() {
        let mut from = empty_snapshot();
        from.enums.insert(
            "old_status".to_string(),
            EnumSnapshot {
                name: "old_status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );

        let mut to = empty_snapshot();
        to.enums.insert(
            "new_status".to_string(),
            EnumSnapshot {
                name: "new_status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );

        // Without rename decision: should drop old and create new
        let diff = diff_snapshots(&from, &to).unwrap();
        assert_eq!(diff.statements.len(), 2);

        // With rename decision: should rename instead
        let mut renames = RenameDecisions::new();
        renames.enums.insert(
            "old_status".to_string(),
            RenameDecision::Rename {
                from: "old_status".to_string(),
                to: "new_status".to_string(),
            },
        );

        let diff_with_rename = apply_renames(&diff, &renames);
        assert_eq!(diff_with_rename.statements.len(), 1);
        match &diff_with_rename.statements[0] {
            DiffStatement::RenameEnum { from, to, .. } => {
                assert_eq!(from, "old_status");
                assert_eq!(to, "new_status");
            }
            _ => panic!("Expected RenameEnum, got {:?}", diff_with_rename.statements[0]),
        }
    }

    #[test]
    fn test_diff_renamed_column_with_type_change() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "old_col".to_string(),
            ColumnSnapshot {
                name: "old_col".to_string(),
                data_type: "varchar(50)".to_string(),
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "new_col".to_string(),
            ColumnSnapshot {
                name: "new_col".to_string(),
                data_type: "varchar(100)".to_string(), // Type change
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        to.tables.insert("users".to_string(), table_to);

        // With rename decision: should rename and also alter type
        let mut renames = RenameDecisions::new();
        renames.columns.insert(
            ("users".to_string(), "old_col".to_string()),
            RenameDecision::Rename {
                from: "old_col".to_string(),
                to: "new_col".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff_with_rename = apply_renames(&diff, &renames);
        assert_eq!(diff_with_rename.statements.len(), 2);
        
        let has_rename = diff_with_rename.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { from, to, .. } if from == "old_col" && to == "new_col")
        });
        let has_alter = diff_with_rename.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumn { column, changes, .. } if column == "new_col" && !changes.is_empty())
        });
        
        assert!(has_rename, "Expected RenameColumn");
        assert!(has_alter, "Expected AlterColumn for type change");
    }

    #[test]
    fn test_diff_multiple_column_renames_in_same_table() {
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert("col_a".to_string(), create_column_snapshot("col_a", None));
        table_from.columns.insert("col_b".to_string(), create_column_snapshot("col_b", None));
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert("renamed_a".to_string(), create_column_snapshot("renamed_a", None));
        table_to.columns.insert("renamed_b".to_string(), create_column_snapshot("renamed_b", None));
        to.tables.insert("users".to_string(), table_to);

        // Create rename decisions for both columns
        let mut renames = RenameDecisions::new();
        renames.columns.insert(
            ("users".to_string(), "col_a".to_string()),
            RenameDecision::Rename {
                from: "col_a".to_string(),
                to: "renamed_a".to_string(),
            },
        );
        renames.columns.insert(
            ("users".to_string(), "col_b".to_string()),
            RenameDecision::Rename {
                from: "col_b".to_string(),
                to: "renamed_b".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff_with_rename = apply_renames(&diff, &renames);
        
        // Should have exactly 2 rename statements
        let rename_count = diff_with_rename.statements.iter()
            .filter(|s| matches!(s, DiffStatement::RenameColumn { .. }))
            .count();
        assert_eq!(rename_count, 2);
    }

    #[test]
    fn test_diff_partial_rename_decisions() {
        // Test case where user renames one column but drops another
        let mut from = empty_snapshot();
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert("col_a".to_string(), create_column_snapshot("col_a", None));
        table_from.columns.insert("col_b".to_string(), create_column_snapshot("col_b", None));
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert("renamed_a".to_string(), create_column_snapshot("renamed_a", None));
        table_to.columns.insert("new_c".to_string(), create_column_snapshot("new_c", None));
        to.tables.insert("users".to_string(), table_to);

        // Only rename col_a -> renamed_a, let col_b be dropped and new_c be added
        let mut renames = RenameDecisions::new();
        renames.columns.insert(
            ("users".to_string(), "col_a".to_string()),
            RenameDecision::Rename {
                from: "col_a".to_string(),
                to: "renamed_a".to_string(),
            },
        );
        // col_b -> new_c is NOT a rename (user chose to drop/add)

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        let has_rename_a = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { from, to, .. } if from == "col_a" && to == "renamed_a")
        });
        let has_drop_b = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::DropColumn { column, .. } if column == "col_b")
        });
        let has_add_c = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AddColumn { column, .. } if column.name == "new_c")
        });
        
        assert!(has_rename_a, "Expected RenameColumn for col_a");
        assert!(has_drop_b, "Expected DropColumn for col_b");
        assert!(has_add_c, "Expected AddColumn for new_c");
    }

    #[test]
    fn test_diff_with_table_and_column_rename() {
        // This tests the scenario where both a table and columns within it are renamed.
        // The column rename decisions should be keyed by the ORIGINAL table name.
        let mut from = empty_snapshot();
        let mut old_table = create_table_snapshot("old_users", None);
        old_table.columns.insert(
            "old_email".to_string(),
            create_column_snapshot("old_email", None),
        );
        old_table.columns.insert(
            "old_name".to_string(),
            create_column_snapshot("old_name", None),
        );
        from.tables.insert("old_users".to_string(), old_table);

        let mut to = empty_snapshot();
        let mut new_table = create_table_snapshot("new_accounts", None);
        new_table.columns.insert(
            "new_email".to_string(),
            create_column_snapshot("new_email", None),
        );
        new_table.columns.insert(
            "new_name".to_string(),
            create_column_snapshot("new_name", None),
        );
        to.tables.insert("new_accounts".to_string(), new_table);

        // Create rename decisions: table rename + column renames
        // The column renames should be keyed by the ORIGINAL table name (old_users)
        let mut renames = RenameDecisions::new();
        renames.tables.insert(
            "old_users".to_string(),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_accounts".to_string(),
            },
        );
        // Column renames - keyed by ORIGINAL table name
        renames.columns.insert(
            ("old_users".to_string(), "old_email".to_string()),
            RenameDecision::Rename {
                from: "old_email".to_string(),
                to: "new_email".to_string(),
            },
        );
        renames.columns.insert(
            ("old_users".to_string(), "old_name".to_string()),
            RenameDecision::Rename {
                from: "old_name".to_string(),
                to: "new_name".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should have: 1 table rename + 2 column renames = 3 statements
        assert_eq!(diff.statements.len(), 3, "Expected 3 statements, got: {:?}", diff.statements);
        
        let has_table_rename = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameTable { from, to, .. } if from == "old_users" && to == "new_accounts")
        });
        // Column renames use ORIGINAL table name (before table rename)
        let has_email_rename = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { table, from, to, .. } 
                if table == "old_users" && from == "old_email" && to == "new_email")
        });
        let has_name_rename = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { table, from, to, .. }
                if table == "old_users" && from == "old_name" && to == "new_name")
        });
        
        assert!(has_table_rename, "Expected RenameTable from old_users to new_accounts");
        assert!(has_email_rename, "Expected RenameColumn from old_email to new_email in old_users");
        assert!(has_name_rename, "Expected RenameColumn from old_name to new_name in old_users");
        
        // Verify no drops or adds
        let has_any_drop = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::DropTable { .. } | DiffStatement::DropColumn { .. })
        });
        let has_any_add = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::CreateTable { .. } | DiffStatement::AddColumn { .. })
        });
        
        assert!(!has_any_drop, "Should not have any drop statements when renaming");
        assert!(!has_any_add, "Should not have any add statements when renaming");
    }

    #[test]
    fn test_diff_with_table_rename_partial_column_rename() {
        // Table is renamed, one column is renamed, another column is dropped+added
        let mut from = empty_snapshot();
        let mut old_table = create_table_snapshot("old_users", None);
        old_table.columns.insert(
            "old_email".to_string(),
            create_column_snapshot("old_email", None),
        );
        old_table.columns.insert(
            "dropped_col".to_string(),
            create_column_snapshot("dropped_col", None),
        );
        from.tables.insert("old_users".to_string(), old_table);

        let mut to = empty_snapshot();
        let mut new_table = create_table_snapshot("new_accounts", None);
        new_table.columns.insert(
            "new_email".to_string(),
            create_column_snapshot("new_email", None),
        );
        new_table.columns.insert(
            "added_col".to_string(),
            create_column_snapshot("added_col", None),
        );
        to.tables.insert("new_accounts".to_string(), new_table);

        // Table is renamed, only one column is renamed
        let mut renames = RenameDecisions::new();
        renames.tables.insert(
            "old_users".to_string(),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_accounts".to_string(),
            },
        );
        renames.columns.insert(
            ("old_users".to_string(), "old_email".to_string()),
            RenameDecision::Rename {
                from: "old_email".to_string(),
                to: "new_email".to_string(),
            },
        );
        // dropped_col -> added_col is NOT a rename

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should have: 1 table rename + 1 column rename + 1 drop + 1 add = 4 statements
        assert_eq!(diff.statements.len(), 4, "Expected 4 statements, got: {:?}", diff.statements);
        
        let has_table_rename = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameTable { from, to, .. } if from == "old_users" && to == "new_accounts")
        });
        let has_email_rename = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { from, to, .. } if from == "old_email" && to == "new_email")
        });
        let has_drop = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::DropColumn { column, .. } if column == "dropped_col")
        });
        let has_add = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AddColumn { column, .. } if column.name == "added_col")
        });
        
        assert!(has_table_rename, "Expected RenameTable");
        assert!(has_email_rename, "Expected RenameColumn for email");
        assert!(has_drop, "Expected DropColumn for dropped_col");
        assert!(has_add, "Expected AddColumn for added_col");
    }

    #[test]
    fn test_diff_column_with_renamed_enum_type() {
        // When an enum is renamed, columns using that enum type should NOT generate
        // an AlterColumn SetType, because PostgreSQL handles this automatically
        let mut from = empty_snapshot();
        from.enums.insert(
            "old_status".to_string(),
            EnumSnapshot {
                name: "old_status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "old_status".to_string(), // Uses the old enum name
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        to.enums.insert(
            "new_status".to_string(),
            EnumSnapshot {
                name: "new_status".to_string(),
                schema: None,
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "new_status".to_string(), // Uses the new enum name
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        to.tables.insert("users".to_string(), table_to);

        // Create rename decision for the enum
        let mut renames = RenameDecisions::new();
        renames.enums.insert(
            "old_status".to_string(),
            RenameDecision::Rename {
                from: "old_status".to_string(),
                to: "new_status".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should only have the RenameEnum statement, no AlterColumn
        assert_eq!(diff.statements.len(), 1, "Expected 1 statement (RenameEnum), got: {:?}", diff.statements);
        
        match &diff.statements[0] {
            DiffStatement::RenameEnum { from, to, .. } => {
                assert_eq!(from, "old_status");
                assert_eq!(to, "new_status");
            }
            _ => panic!("Expected RenameEnum, got: {:?}", diff.statements[0]),
        }
        
        // Verify no AlterColumn was generated
        let has_alter_column = diff.statements.iter().any(|s| matches!(s, DiffStatement::AlterColumn { .. }));
        assert!(!has_alter_column, "Should not have AlterColumn when enum is renamed");
    }

    #[test]
    fn test_diff_column_with_renamed_enum_array_type() {
        // Same as above, but with array type: old_status[] -> new_status[]
        let mut from = empty_snapshot();
        from.enums.insert(
            "old_status".to_string(),
            EnumSnapshot {
                name: "old_status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: None,
            },
        );
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "statuses".to_string(),
            ColumnSnapshot {
                name: "statuses".to_string(),
                data_type: "old_status[]".to_string(), // Array of old enum
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        to.enums.insert(
            "new_status".to_string(),
            EnumSnapshot {
                name: "new_status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: None,
            },
        );
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "statuses".to_string(),
            ColumnSnapshot {
                name: "statuses".to_string(),
                data_type: "new_status[]".to_string(), // Array of new enum
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        to.tables.insert("users".to_string(), table_to);

        // Create rename decision for the enum
        let mut renames = RenameDecisions::new();
        renames.enums.insert(
            "old_status".to_string(),
            RenameDecision::Rename {
                from: "old_status".to_string(),
                to: "new_status".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should only have the RenameEnum statement, no AlterColumn
        assert_eq!(diff.statements.len(), 1, "Expected 1 statement, got: {:?}", diff.statements);
        assert!(matches!(&diff.statements[0], DiffStatement::RenameEnum { .. }));
    }

    #[test]
    fn test_diff_column_with_schema_qualified_renamed_enum() {
        // Test with schema-qualified enum type: public.old_status -> public.new_status
        let mut from = empty_snapshot();
        from.enums.insert(
            "old_status".to_string(),
            EnumSnapshot {
                name: "old_status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["active".to_string()],
                description: None,
            },
        );
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "public.old_status".to_string(), // Schema-qualified
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        to.enums.insert(
            "new_status".to_string(),
            EnumSnapshot {
                name: "new_status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["active".to_string()],
                description: None,
            },
        );
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "public.new_status".to_string(), // Schema-qualified new name
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        to.tables.insert("users".to_string(), table_to);

        // Create rename decision for the enum
        let mut renames = RenameDecisions::new();
        renames.enums.insert(
            "old_status".to_string(),
            RenameDecision::Rename {
                from: "old_status".to_string(),
                to: "new_status".to_string(),
            },
        );

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should only have the RenameEnum statement, no AlterColumn
        assert_eq!(diff.statements.len(), 1, "Expected 1 statement, got: {:?}", diff.statements);
        assert!(matches!(&diff.statements[0], DiffStatement::RenameEnum { .. }));
    }

    #[test]
    fn test_diff_column_real_type_change_not_confused_with_rename() {
        // Ensure actual type changes are not confused with enum renames
        let mut from = empty_snapshot();
        from.enums.insert(
            "old_status".to_string(),
            EnumSnapshot {
                name: "old_status".to_string(),
                schema: None,
                values: vec!["active".to_string()],
                description: None,
            },
        );
        let mut table_from = create_table_snapshot("users", None);
        table_from.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "old_status".to_string(),
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        from.tables.insert("users".to_string(), table_from);

        let mut to = empty_snapshot();
        // Different enum entirely (not a rename target)
        to.enums.insert(
            "different_enum".to_string(),
            EnumSnapshot {
                name: "different_enum".to_string(),
                schema: None,
                values: vec!["foo".to_string()],
                description: None,
            },
        );
        let mut table_to = create_table_snapshot("users", None);
        table_to.columns.insert(
            "status".to_string(),
            ColumnSnapshot {
                name: "status".to_string(),
                data_type: "different_enum".to_string(), // Changed to completely different type
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        to.tables.insert("users".to_string(), table_to);

        // No rename decisions - this is a real type change
        let renames = RenameDecisions::new();

        let diff = diff_snapshots(&from, &to).unwrap();
        let diff = apply_renames(&diff, &renames);
        
        // Should have: DropEnum, CreateEnum, AlterColumn
        let has_alter = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumn { changes, .. } 
                if changes.iter().any(|c| matches!(c, ColumnChange::SetType(t) if t == "different_enum")))
        });
        assert!(has_alter, "Expected AlterColumn with SetType for real type change. Got: {:?}", diff.statements);
    }
}
