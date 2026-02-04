//! Schema diffing algorithm
//!
//! This module computes the difference between two schema snapshots and produces
//! a list of statements needed to migrate from one to the other.

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
    CreateSchema(CreateSchemaStmt),
    DropSchema(DropSchemaStmt),
    RenameSchema(RenameSchemaStmt),

    // Enum operations
    CreateEnum(CreateEnumStmt),
    DropEnum(DropEnumStmt),
    RenameEnum(RenameEnumStmt),
    AddEnumValue(AddEnumValueStmt),
    AlterEnumDescription(AlterEnumDescriptionStmt),

    // Sequence operations
    CreateSequence(CreateSequenceStmt),
    DropSequence(DropSequenceStmt),
    AlterSequence(AlterSequenceStmt),

    // Table operations
    CreateTable(CreateTableStmt),
    DropTable(DropTableStmt),
    RenameTable(RenameTableStmt),
    AlterTableComment(AlterTableCommentStmt),

    // Column operations
    AddColumn(AddColumnStmt),
    DropColumn(DropColumnStmt),
    RenameColumn(RenameColumnStmt),
    AlterColumn(AlterColumnStmt),
    AlterColumnComment(AlterColumnCommentStmt),

    // Index operations
    CreateIndex(CreateIndexStmt),
    DropIndex(DropIndexStmt),

    // Constraint operations
    AddConstraint(AddConstraintStmt),
    DropConstraint(DropConstraintStmt),

    // View operations
    CreateView(CreateViewStmt),
    DropView(DropViewStmt),
    AlterView(AlterViewStmt),

    // Extension operations (PostgreSQL)
    CreateExtension(String),
    DropExtension(String),
}

// Statement structs
#[derive(Debug, Clone)]
pub struct CreateSchemaStmt {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DropSchemaStmt {
    pub name: String,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct RenameSchemaStmt {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct CreateEnumStmt {
    pub name: String,
    pub schema: Option<String>,
    pub values: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DropEnumStmt {
    pub name: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenameEnumStmt {
    pub from: String,
    pub to: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddEnumValueStmt {
    pub enum_name: String,
    pub schema: Option<String>,
    pub value: String,
    pub position: EnumValuePosition,
}

#[derive(Debug, Clone)]
pub enum EnumValuePosition {
    End,
    Before(String),
    After(String),
}

#[derive(Debug, Clone)]
pub struct AlterEnumDescriptionStmt {
    pub name: String,
    pub schema: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateSequenceStmt {
    pub sequence: SequenceSnapshot,
}

#[derive(Debug, Clone)]
pub struct DropSequenceStmt {
    pub name: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlterSequenceStmt {
    pub name: String,
    pub schema: Option<String>,
    pub changes: Vec<SequenceChange>,
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
pub struct CreateTableStmt {
    pub table: TableSnapshot,
}

#[derive(Debug, Clone)]
pub struct DropTableStmt {
    pub name: String,
    pub schema: Option<String>,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct RenameTableStmt {
    pub from: String,
    pub to: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlterTableCommentStmt {
    pub table: String,
    pub schema: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddColumnStmt {
    pub table: String,
    pub schema: Option<String>,
    pub column: ColumnSnapshot,
}

#[derive(Debug, Clone)]
pub struct DropColumnStmt {
    pub table: String,
    pub schema: Option<String>,
    pub column: String,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct RenameColumnStmt {
    pub table: String,
    pub schema: Option<String>,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct AlterColumnStmt {
    pub table: String,
    pub schema: Option<String>,
    pub column: String,
    pub changes: Vec<ColumnChange>,
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

#[derive(Debug, Clone)]
pub struct AlterColumnCommentStmt {
    pub table: String,
    pub schema: Option<String>,
    pub column: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateIndexStmt {
    pub table: String,
    pub schema: Option<String>,
    pub index: IndexSnapshot,
    pub concurrently: bool,
    pub if_not_exists: bool,
}

#[derive(Debug, Clone)]
pub struct DropIndexStmt {
    pub name: String,
    pub schema: Option<String>,
    pub concurrently: bool,
    pub if_exists: bool,
}

#[derive(Debug, Clone)]
pub struct AddConstraintStmt {
    pub table: String,
    pub schema: Option<String>,
    pub constraint: ConstraintSnapshot,
}

#[derive(Debug, Clone)]
pub struct DropConstraintStmt {
    pub table: String,
    pub schema: Option<String>,
    pub name: String,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct CreateViewStmt {
    pub view: ViewSnapshot,
    pub or_replace: bool,
}

#[derive(Debug, Clone)]
pub struct DropViewStmt {
    pub name: String,
    pub schema: Option<String>,
    pub materialized: bool,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct AlterViewStmt {
    pub name: String,
    pub schema: Option<String>,
    pub new_definition: String,
}

/// Compute the diff between two snapshots
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
            statements.push(DiffStatement::CreateSchema(CreateSchemaStmt {
                name: schema.clone(),
            }));
        }
    }

    // Schemas to drop
    for schema in from {
        if !to.contains(schema) && schema != "public" && schema != "main" {
            statements.push(DiffStatement::DropSchema(DropSchemaStmt {
                name: schema.clone(),
                cascade: false,
            }));
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
            statements.push(DiffStatement::CreateEnum(CreateEnumStmt {
                name: enum_to.name.clone(),
                schema: enum_to.schema.clone(),
                values: enum_to.values.clone(),
                description: enum_to.description.clone(),
            }));
        }
    }

    // Enums to drop
    for (name, enum_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropEnum(DropEnumStmt {
                name: enum_from.name.clone(),
                schema: enum_from.schema.clone(),
            }));
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
                    statements.push(DiffStatement::AddEnumValue(AddEnumValueStmt {
                        enum_name: enum_to.name.clone(),
                        schema: enum_to.schema.clone(),
                        value: value.clone(),
                        position,
                    }));
                }
                prev_value = Some(value);
            }

            // Check for description changes
            if enum_from.description != enum_to.description {
                statements.push(DiffStatement::AlterEnumDescription(
                    AlterEnumDescriptionStmt {
                        name: enum_to.name.clone(),
                        schema: enum_to.schema.clone(),
                        description: enum_to.description.clone(),
                    },
                ));
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
            statements.push(DiffStatement::CreateSequence(CreateSequenceStmt {
                sequence: seq_to.clone(),
            }));
        }
    }

    // Sequences to drop
    for (name, seq_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropSequence(DropSequenceStmt {
                name: seq_from.name.clone(),
                schema: seq_from.schema.clone(),
            }));
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
                statements.push(DiffStatement::AlterSequence(AlterSequenceStmt {
                    name: seq_to.name.clone(),
                    schema: seq_to.schema.clone(),
                    changes,
                }));
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
            statements.push(DiffStatement::CreateTable(CreateTableStmt {
                table: table_to.clone(),
            }));
        }
    }

    // Tables to drop
    for (name, table_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropTable(DropTableStmt {
                name: table_from.name.clone(),
                schema: table_from.schema.clone(),
                cascade: false,
            }));
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
        statements.push(DiffStatement::AlterTableComment(AlterTableCommentStmt {
            table: table.clone(),
            schema: schema.clone(),
            comment: to.comment.clone(),
        }));
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
            statements.push(DiffStatement::AddColumn(AddColumnStmt {
                table: table.to_string(),
                schema: schema.clone(),
                column: col_to.clone(),
            }));
        }
    }

    // Columns to drop
    for (name, _col_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropColumn(DropColumnStmt {
                table: table.to_string(),
                schema: schema.clone(),
                column: name.clone(),
                cascade: false,
            }));
        }
    }

    // Columns to alter
    for (name, col_to) in to {
        if let Some(col_from) = from.get(name) {
            let changes = diff_column(col_from, col_to);
            if !changes.is_empty() {
                statements.push(DiffStatement::AlterColumn(AlterColumnStmt {
                    table: table.to_string(),
                    schema: schema.clone(),
                    column: name.clone(),
                    changes,
                }));
            }

            // Check for comment changes (handled separately from other column changes)
            if col_from.comment != col_to.comment {
                statements.push(DiffStatement::AlterColumnComment(AlterColumnCommentStmt {
                    table: table.to_string(),
                    schema: schema.clone(),
                    column: name.clone(),
                    comment: col_to.comment.clone(),
                }));
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
            statements.push(DiffStatement::CreateIndex(CreateIndexStmt {
                table: table.to_string(),
                schema: schema.clone(),
                index: idx_to.clone(),
                concurrently: false,
                if_not_exists: false,
            }));
        }
    }

    // Indexes to drop
    for (name, _idx_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropIndex(DropIndexStmt {
                name: name.clone(),
                schema: schema.clone(),
                concurrently: false,
                if_exists: false,
            }));
        }
    }

    // Indexes to recreate (if changed)
    for (name, idx_to) in to {
        if let Some(idx_from) = from.get(name)
            && idx_from != idx_to
        {
            // Drop and recreate
            statements.push(DiffStatement::DropIndex(DropIndexStmt {
                name: name.clone(),
                schema: schema.clone(),
                concurrently: false,
                if_exists: false,
            }));
            statements.push(DiffStatement::CreateIndex(CreateIndexStmt {
                table: table.to_string(),
                schema: schema.clone(),
                index: idx_to.clone(),
                concurrently: false,
                if_not_exists: false,
            }));
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
            statements.push(DiffStatement::AddConstraint(AddConstraintStmt {
                table: table.to_string(),
                schema: schema.clone(),
                constraint: (*constraint).clone(),
            }));
        }
    }

    // Constraints to drop
    for (name, _constraint) in &from_by_name {
        if !to_by_name.contains_key(name) {
            statements.push(DiffStatement::DropConstraint(DropConstraintStmt {
                table: table.to_string(),
                schema: schema.clone(),
                name: name.clone(),
                cascade: false,
            }));
        }
    }

    // Constraints to modify (drop and recreate)
    for (name, constraint_to) in &to_by_name {
        if let Some(constraint_from) = from_by_name.get(name)
            && constraint_from != constraint_to
        {
            statements.push(DiffStatement::DropConstraint(DropConstraintStmt {
                table: table.to_string(),
                schema: schema.clone(),
                name: name.clone(),
                cascade: false,
            }));
            statements.push(DiffStatement::AddConstraint(AddConstraintStmt {
                table: table.to_string(),
                schema: schema.clone(),
                constraint: (*constraint_to).clone(),
            }));
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
            statements.push(DiffStatement::CreateView(CreateViewStmt {
                view: view_to.clone(),
                or_replace: false,
            }));
        }
    }

    // Views to drop
    for (name, view_from) in from {
        if !to.contains_key(name) {
            statements.push(DiffStatement::DropView(DropViewStmt {
                name: view_from.name.clone(),
                schema: view_from.schema.clone(),
                materialized: view_from.materialized,
                cascade: false,
            }));
        }
    }

    // Views to alter
    for (name, view_to) in to {
        if let Some(view_from) = from.get(name)
            && view_from.definition != view_to.definition
        {
            statements.push(DiffStatement::AlterView(AlterViewStmt {
                name: view_to.name.clone(),
                schema: view_to.schema.clone(),
                new_definition: view_to.definition.clone(),
            }));
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

    /// Check if the diff contains any destructive operations
    pub fn has_destructive_changes(&self) -> bool {
        self.statements.iter().any(|s| {
            matches!(
                s,
                DiffStatement::DropSchema(_)
                    | DiffStatement::DropEnum(_)
                    | DiffStatement::DropSequence(_)
                    | DiffStatement::DropTable(_)
                    | DiffStatement::DropColumn(_)
                    | DiffStatement::DropView(_)
            )
        })
    }

    /// Get a summary of changes
    pub fn summary(&self) -> DiffSummary {
        let mut summary = DiffSummary::default();

        for stmt in &self.statements {
            match stmt {
                DiffStatement::CreateSchema(_) => summary.schemas_created += 1,
                DiffStatement::DropSchema(_) => summary.schemas_dropped += 1,
                DiffStatement::CreateEnum(_) => summary.enums_created += 1,
                DiffStatement::DropEnum(_) => summary.enums_dropped += 1,
                DiffStatement::AddEnumValue(_) => summary.enum_values_added += 1,
                DiffStatement::AlterEnumDescription(_) => summary.enums_altered += 1,
                DiffStatement::CreateSequence(_) => summary.sequences_created += 1,
                DiffStatement::DropSequence(_) => summary.sequences_dropped += 1,
                DiffStatement::AlterSequence(_) => summary.sequences_altered += 1,
                DiffStatement::CreateTable(_) => summary.tables_created += 1,
                DiffStatement::DropTable(_) => summary.tables_dropped += 1,
                DiffStatement::RenameTable(_) => summary.tables_renamed += 1,
                DiffStatement::AlterTableComment(_) => summary.tables_altered += 1,
                DiffStatement::AddColumn(_) => summary.columns_added += 1,
                DiffStatement::DropColumn(_) => summary.columns_dropped += 1,
                DiffStatement::RenameColumn(_) => summary.columns_renamed += 1,
                DiffStatement::AlterColumn(_) => summary.columns_altered += 1,
                DiffStatement::AlterColumnComment(_) => summary.columns_altered += 1,
                DiffStatement::CreateIndex(_) => summary.indexes_created += 1,
                DiffStatement::DropIndex(_) => summary.indexes_dropped += 1,
                DiffStatement::AddConstraint(_) => summary.constraints_added += 1,
                DiffStatement::DropConstraint(_) => summary.constraints_dropped += 1,
                DiffStatement::CreateView(_) => summary.views_created += 1,
                DiffStatement::DropView(_) => summary.views_dropped += 1,
                DiffStatement::AlterView(_) => summary.views_altered += 1,
                DiffStatement::CreateExtension(_) => summary.extensions_created += 1,
                DiffStatement::DropExtension(_) => summary.extensions_dropped += 1,
                DiffStatement::RenameSchema(_) => summary.schemas_renamed += 1,
                DiffStatement::RenameEnum(_) => summary.enums_renamed += 1,
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
    use crate::schema::SchemaDialect;

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
            DiffStatement::AlterEnumDescription(stmt) => {
                assert_eq!(stmt.name, "status");
                assert_eq!(stmt.description, Some("Status of the entity".to_string()));
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
            DiffStatement::AlterEnumDescription(stmt) => {
                assert_eq!(stmt.name, "status");
                assert_eq!(stmt.description, Some("New description".to_string()));
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
            DiffStatement::AlterEnumDescription(stmt) => {
                assert_eq!(stmt.name, "status");
                assert_eq!(stmt.description, None);
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
            .any(|s| matches!(s, DiffStatement::AddEnumValue(stmt) if stmt.value == "pending"));
        let has_alter_desc = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterEnumDescription(stmt) if stmt.description == Some("New description".to_string()))
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
            DiffStatement::AlterTableComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.comment, Some("User accounts table".to_string()));
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
            DiffStatement::AlterTableComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.comment, Some("New comment".to_string()));
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
            DiffStatement::AlterTableComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.comment, None);
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
            DiffStatement::AlterColumnComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.column, "email");
                assert_eq!(stmt.comment, Some("User email address".to_string()));
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
            DiffStatement::AlterColumnComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.column, "email");
                assert_eq!(stmt.comment, Some("New comment".to_string()));
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
            DiffStatement::AlterColumnComment(stmt) => {
                assert_eq!(stmt.table, "users");
                assert_eq!(stmt.column, "email");
                assert_eq!(stmt.comment, None);
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
            matches!(s, DiffStatement::AlterTableComment(stmt) if stmt.comment == Some("New table comment".to_string()))
        });
        let has_column_comment = diff.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumnComment(stmt) if stmt.comment == Some("New column comment".to_string()))
        });

        assert!(has_table_comment, "Expected AlterTableComment statement");
        assert!(has_column_comment, "Expected AlterColumnComment statement");
    }
}
