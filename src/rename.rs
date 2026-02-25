//! Rename detection for schema diffs
//!
//! This module detects potential renames from a SchemaDiff and prompts the user
//! to decide whether add/drop pairs should be treated as renames (preserving data)
//! or kept as actual add/drop operations.
//!
//! The detection happens in phases to properly handle cross-entity relationships:
//! 1. First, table and enum renames are detected and prompted
//! 2. Then, column renames are detected considering table rename decisions
//!    (e.g., columns in a renamed table are properly tracked)

use colored::Colorize;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use indexmap::IndexMap;
use std::fmt;

use crate::diff::{ColumnChange, DiffStatement, SchemaDiff};
use crate::snapshot::{ColumnSnapshot, EnumSnapshot, TableSnapshot};
use crate::{Result, ShkiError};

// ============================================================================
// TableId - combines table name and optional schema
// ============================================================================

/// Identifier for a table, combining name and optional schema
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableId {
    name: String,
    schema: Option<String>,
}

impl TableId {
    pub fn new(name: impl Into<String>, schema: Option<String>) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }

    fn with_name(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: self.schema.clone(),
        }
    }

    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn column(&self, name: impl Into<String>) -> ColumnId {
        ColumnId::new(self.clone(), name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnId {
    table: TableId,
    name: String,
}

impl ColumnId {
    pub fn new(table: TableId, name: impl Into<String>) -> Self {
        Self {
            table,
            name: name.into(),
        }
    }

    pub fn table(&self) -> &TableId {
        &self.table
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl From<&TableSnapshot> for TableId {
    fn from(table: &TableSnapshot) -> Self {
        Self {
            name: table.name.clone(),
            schema: table.schema.clone(),
        }
    }
}

impl fmt::Display for TableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.schema {
            Some(s) => write!(f, "{}.{}", s, self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

/// Format a schema-qualified name for display
fn format_qualified_name(name: &str, schema: &Option<String>) -> String {
    schema
        .as_deref()
        .map(|s| format!("{}.{}", s, name))
        .unwrap_or_else(|| name.to_owned())
}

// ============================================================================
// RenameDecision & RenameDecisions
// ============================================================================

/// The user's decision for a rename candidate
#[derive(Debug, Clone, PartialEq)]
pub enum RenameDecision {
    /// Keep as separate add and drop operations
    KeepAddDrop,
    /// Rename the entity
    Rename { from: String, to: String },
}

/// Collection of rename decisions made by the user
#[derive(Debug, Clone, Default)]
pub struct RenameDecisions {
    /// Table renames: key is dropped table id
    pub tables: IndexMap<TableId, RenameDecision>,
    /// Column renames: key is dropped column id
    pub columns: IndexMap<ColumnId, RenameDecision>,
    /// Enum renames: key is dropped enum id
    pub enums: IndexMap<TableId, RenameDecision>,
}

impl RenameDecisions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_table_decision(&mut self, dropped: &TableId, decision: RenameDecision) {
        self.tables.insert(dropped.clone(), decision);
    }

    pub fn insert_column_decision(&mut self, dropped: &ColumnId, decision: RenameDecision) {
        self.columns.insert(dropped.clone(), decision);
    }

    pub fn insert_enum_decision(&mut self, dropped: &TableId, decision: RenameDecision) {
        self.enums.insert(dropped.clone(), decision);
    }

    /// Get the new name if a table was renamed
    pub fn get_table_new_name(&self, dropped: &TableId) -> Option<&str> {
        self.tables
            .get(dropped)
            .and_then(|decision| match decision {
                RenameDecision::Rename { to, .. } => Some(to.as_str()),
                _ => None,
            })
    }

    /// Get the original table name for a given new table name (if it was renamed)
    pub fn get_table_old_name<'a>(&'a self, added: &TableId) -> Option<&'a str> {
        self.tables.iter().find_map(|(dropped, decision)| {
            if let RenameDecision::Rename { to, .. } = decision
                && dropped.schema() == added.schema()
                && to == added.name()
            {
                return Some(dropped.name());
            }
            None
        })
    }

    /// Get the new name if a column was renamed (keyed by original table name)
    pub fn get_column_new_name(&self, dropped: &ColumnId) -> Option<&str> {
        self.columns
            .get(dropped)
            .and_then(|decision| match decision {
                RenameDecision::Rename { to, .. } => Some(to.as_str()),
                _ => None,
            })
    }

    /// Get the new name if an enum was renamed
    pub fn get_enum_new_name(&self, dropped: &TableId) -> Option<&str> {
        self.enums.get(dropped).and_then(|decision| match decision {
            RenameDecision::Rename { to, .. } => Some(to.as_str()),
            _ => None,
        })
    }

    /// Check if a table is the target of a rename
    pub fn is_table_renamed(&self, added: &TableId) -> bool {
        self.tables.iter().any(|(dropped, d)| {
            dropped.schema() == added.schema()
                && matches!(d, RenameDecision::Rename { to, .. } if to == added.name())
        })
    }

    /// Check if a column is the target of a rename
    pub fn is_column_renamed(&self, added: &ColumnId) -> bool {
        self.columns.iter().any(|(dropped, d)| {
            dropped.table() == added.table()
                && matches!(d, RenameDecision::Rename { to, .. } if to == added.name())
        })
    }

    /// Check if an enum is the target of a rename
    pub fn is_enum_renamed(&self, added: &TableId) -> bool {
        self.enums.iter().any(|(dropped, d)| {
            dropped.schema() == added.schema()
                && matches!(d, RenameDecision::Rename { to, .. } if to == added.name())
        })
    }

    /// Check if there are any rename decisions
    pub fn has_renames(&self) -> bool {
        let has_rename = |d: &RenameDecision| matches!(d, RenameDecision::Rename { .. });
        self.tables.values().any(has_rename)
            || self.columns.values().any(has_rename)
            || self.enums.values().any(has_rename)
    }

    /// Get the effective table name (after any rename)
    fn effective_table_name(&self, original: &TableId) -> String {
        self.get_table_new_name(original)
            .unwrap_or(original.name())
            .to_owned()
    }
}

// ============================================================================
// Changes - unified diff analysis (single-pass extraction)
// ============================================================================

/// Grouped changes extracted from a SchemaDiff in a single pass
struct Changes {
    tables: EntityChanges<TableSnapshot>,
    enums: EntityChanges<EnumSnapshot>,
    /// Column changes grouped by original table id
    columns: IndexMap<TableId, TableColumnChanges>,
}

struct EntityChanges<T> {
    dropped: Vec<T>,
    added: Vec<T>,
}

impl<T> EntityChanges<T> {
    fn new() -> Self {
        Self {
            dropped: Vec::new(),
            added: Vec::new(),
        }
    }

    fn has_potential_renames(&self) -> bool {
        !self.dropped.is_empty() && !self.added.is_empty()
    }
}

/// Column changes for a single table
struct TableColumnChanges {
    schema: Option<String>,
    dropped: Vec<ColumnSnapshot>,
    added: Vec<ColumnSnapshot>,
}

impl TableColumnChanges {
    fn new(schema: Option<String>) -> Self {
        Self {
            schema,
            dropped: Vec::new(),
            added: Vec::new(),
        }
    }

    fn has_potential_renames(&self) -> bool {
        !self.dropped.is_empty() && !self.added.is_empty()
    }
}

impl Changes {
    /// Extract and group all changes from a SchemaDiff in a single pass
    fn from_diff(diff: &SchemaDiff) -> Self {
        let mut tables = EntityChanges::new();
        let mut enums = EntityChanges::new();
        let mut columns: IndexMap<TableId, TableColumnChanges> = IndexMap::new();

        for stmt in &diff.statements {
            match stmt {
                DiffStatement::DropTable { prev, .. } => {
                    tables.dropped.push(prev.clone());
                }
                DiffStatement::CreateTable { table } => {
                    tables.added.push(table.clone());
                }
                DiffStatement::DropEnum { prev, .. } => {
                    enums.dropped.push(prev.clone());
                }
                DiffStatement::CreateEnum {
                    name,
                    schema,
                    values,
                    description,
                } => {
                    enums.added.push(EnumSnapshot {
                        name: name.clone(),
                        schema: schema.clone(),
                        values: values.clone(),
                        description: description.clone(),
                    });
                }
                DiffStatement::DropColumn {
                    table,
                    schema,
                    prev,
                    ..
                } => {
                    let key = TableId::new(table, schema.clone());
                    columns
                        .entry(key)
                        .or_insert_with(|| TableColumnChanges::new(schema.clone()))
                        .dropped
                        .push(prev.clone());
                }
                DiffStatement::AddColumn {
                    table,
                    schema,
                    column,
                } => {
                    let key = TableId::new(table, schema.clone());
                    columns
                        .entry(key)
                        .or_insert_with(|| TableColumnChanges::new(schema.clone()))
                        .added
                        .push(column.clone());
                }
                _ => {}
            }
        }

        Self {
            tables,
            enums,
            columns,
        }
    }

    fn has_potential_renames(&self) -> bool {
        self.tables.has_potential_renames()
            || self.enums.has_potential_renames()
            || self.columns.values().any(|c| c.has_potential_renames())
    }

    fn potential_rename_count(&self) -> usize {
        let table_count = if self.tables.has_potential_renames() {
            self.tables.dropped.len()
        } else {
            0
        };

        let enum_count = if self.enums.has_potential_renames() {
            self.enums.dropped.len()
        } else {
            0
        };

        let column_count: usize = self
            .columns
            .values()
            .filter(|c| c.has_potential_renames())
            .map(|c| c.dropped.len())
            .sum();

        table_count + enum_count + column_count
    }
}

// ============================================================================
// RenameDetector - prompts for rename decisions
// ============================================================================

/// Detects potential renames from a SchemaDiff and prompts the user for decisions
pub struct RenameDetector {
    changes: Changes,
}

impl RenameDetector {
    pub fn new(diff: &SchemaDiff) -> Self {
        Self {
            changes: Changes::from_diff(diff),
        }
    }

    pub fn has_potential_renames(&self) -> bool {
        self.changes.has_potential_renames()
    }

    pub fn potential_rename_count(&self) -> usize {
        self.changes.potential_rename_count()
    }

    /// Prompt the user for rename decisions in order: Tables -> Enums -> Columns
    pub fn prompt_for_decisions(&self, interactive: bool) -> Result<RenameDecisions> {
        let mut decisions = RenameDecisions::new();

        if !interactive {
            return Ok(decisions);
        }

        self.prompt_table_renames(&mut decisions)?;
        self.prompt_enum_renames(&mut decisions)?;
        self.prompt_column_renames(&mut decisions)?;

        Ok(decisions)
    }

    fn prompt_table_renames(&self, decisions: &mut RenameDecisions) -> Result<()> {
        if !self.changes.tables.has_potential_renames() {
            return Ok(());
        }

        println!("\n{}", "Detected potential table renames:".cyan().bold());
        println!(
            "{}",
            "  (Renaming preserves table data, dropping loses all data)".dimmed()
        );

        for dropped in &self.changes.tables.dropped {
            let decision = self.prompt_single_table_rename(dropped, decisions)?;
            decisions.insert_table_decision(&TableId::from(dropped), decision);
        }

        Ok(())
    }

    fn prompt_enum_renames(&self, decisions: &mut RenameDecisions) -> Result<()> {
        if !self.changes.enums.has_potential_renames() {
            return Ok(());
        }

        println!("\n{}", "Detected potential enum renames:".cyan().bold());

        for dropped in &self.changes.enums.dropped {
            let decision = self.prompt_single_enum_rename(dropped, decisions)?;
            decisions.insert_enum_decision(
                &TableId::new(&dropped.name, dropped.schema.clone()),
                decision,
            );
        }

        Ok(())
    }

    fn prompt_column_renames(&self, decisions: &mut RenameDecisions) -> Result<()> {
        for (table_id, col_changes) in &self.changes.columns {
            if !col_changes.has_potential_renames() {
                continue;
            }

            let effective_table = decisions.effective_table_name(table_id);
            let display_name = format_qualified_name(&effective_table, &col_changes.schema);

            println!(
                "\n{} {}",
                "Detected potential column renames in table".cyan().bold(),
                display_name.yellow()
            );
            println!(
                "{}",
                "  (Renaming columns preserves data, while drop+add loses data)".dimmed()
            );

            for dropped in &col_changes.dropped {
                // Filter out columns already chosen as rename targets
                let available: Vec<_> = col_changes
                    .added
                    .iter()
                    .filter(|a| !decisions.is_column_renamed(&table_id.column(&a.name)))
                    .collect();

                if available.is_empty() {
                    decisions.insert_column_decision(
                        &table_id.column(&dropped.name),
                        RenameDecision::KeepAddDrop,
                    );
                    continue;
                }

                let decision =
                    self.prompt_single_column_rename(&effective_table, dropped, &available)?;
                decisions.insert_column_decision(&table_id.column(&dropped.name), decision);
            }
        }

        Ok(())
    }

    fn prompt_single_table_rename(
        &self,
        dropped: &TableSnapshot,
        decisions: &RenameDecisions,
    ) -> Result<RenameDecision> {
        let dropped_name = format_qualified_name(&dropped.name, &dropped.schema);

        let available: Vec<_> = self
            .changes
            .tables
            .added
            .iter()
            .filter(|a| a.schema == dropped.schema)
            .filter(|a| !decisions.is_table_renamed(&TableId::from(*a)))
            .collect();

        if available.is_empty() {
            println!(
                "  {} {} (no available rename targets)",
                "Dropping".red(),
                dropped_name.yellow()
            );
            return Ok(RenameDecision::KeepAddDrop);
        }

        let mut options = vec![format!(
            "{} table {} (all data will be lost)",
            "Drop".red(),
            dropped_name.yellow()
        )];

        for added in &available {
            let added_name = format_qualified_name(&added.name, &added.schema);
            let col_info = format!("{} columns", added.columns.len()).dimmed();
            options.push(format!(
                "{} {} {} {} ({})",
                "Rename".cyan(),
                dropped_name.yellow(),
                "to".dimmed(),
                added_name.green(),
                col_info
            ));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Table {} was dropped. What should happen?",
                dropped_name.yellow()
            ))
            .default(0)
            .items(&options)
            .interact_opt()
            .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?
            .ok_or(ShkiError::Cancelled)?;

        Ok(if selection == 0 {
            RenameDecision::KeepAddDrop
        } else {
            RenameDecision::Rename {
                from: dropped.name.clone(),
                to: available[selection - 1].name.clone(),
            }
        })
    }

    fn prompt_single_enum_rename(
        &self,
        dropped: &EnumSnapshot,
        decisions: &RenameDecisions,
    ) -> Result<RenameDecision> {
        let dropped_name = format_qualified_name(&dropped.name, &dropped.schema);

        let available: Vec<_> = self
            .changes
            .enums
            .added
            .iter()
            .filter(|a| a.schema == dropped.schema)
            .filter(|a| !decisions.is_enum_renamed(&TableId::new(&a.name, a.schema.clone())))
            .collect();

        if available.is_empty() {
            println!(
                "  {} {} (no available rename targets)",
                "Dropping".red(),
                dropped_name.yellow()
            );
            return Ok(RenameDecision::KeepAddDrop);
        }

        let mut options = vec![format!("{} enum {}", "Drop".red(), dropped_name.yellow())];

        for added in &available {
            let added_name = format_qualified_name(&added.name, &added.schema);
            let values_match = if dropped.values == added.values {
                "(same values)".green()
            } else {
                "(values differ)".yellow()
            };
            options.push(format!(
                "{} {} {} {} {}",
                "Rename".cyan(),
                dropped_name.yellow(),
                "to".dimmed(),
                added_name.green(),
                values_match
            ));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Enum {} was dropped. What should happen?",
                dropped_name.yellow()
            ))
            .default(0)
            .items(&options)
            .interact_opt()
            .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?
            .ok_or(ShkiError::Cancelled)?;

        Ok(if selection == 0 {
            RenameDecision::KeepAddDrop
        } else {
            RenameDecision::Rename {
                from: dropped.name.clone(),
                to: available[selection - 1].name.clone(),
            }
        })
    }

    fn prompt_single_column_rename(
        &self,
        table_name: &str,
        dropped: &ColumnSnapshot,
        available: &[&ColumnSnapshot],
    ) -> Result<RenameDecision> {
        let mut options = vec![format!(
            "{} column {} ({}) - data will be lost",
            "Drop".red(),
            dropped.name.yellow(),
            dropped.data_type.dimmed()
        )];

        for added in available {
            let type_info = if dropped.data_type == added.data_type {
                "(same type)".green()
            } else {
                format!("({} -> {})", dropped.data_type, added.data_type).yellow()
            };
            options.push(format!(
                "{} {} {} {} {}",
                "Rename".cyan(),
                dropped.name.yellow(),
                "to".dimmed(),
                added.name.green(),
                type_info
            ));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Column {}.{} was dropped. What should happen?",
                table_name.cyan(),
                dropped.name.yellow()
            ))
            .default(0)
            .items(&options)
            .interact_opt()
            .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?
            .ok_or(ShkiError::Cancelled)?;

        Ok(if selection == 0 {
            RenameDecision::KeepAddDrop
        } else {
            RenameDecision::Rename {
                from: dropped.name.clone(),
                to: available[selection - 1].name.clone(),
            }
        })
    }
}

// ============================================================================
// apply_renames - transform diff based on decisions
// ============================================================================

/// Context for applying renames - holds lookups and decisions
struct RenameContext<'a> {
    renames: &'a RenameDecisions,
    added_columns: IndexMap<ColumnId, &'a ColumnSnapshot>,
    created_tables: IndexMap<TableId, &'a TableSnapshot>,
}

impl<'a> RenameContext<'a> {
    fn new(diff: &'a SchemaDiff, renames: &'a RenameDecisions) -> Self {
        let added_columns = diff
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                DiffStatement::AddColumn {
                    table,
                    schema,
                    column,
                } => Some((
                    ColumnId::new(TableId::new(table, schema.clone()), &column.name),
                    column,
                )),
                _ => None,
            })
            .collect();

        let created_tables = diff
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                DiffStatement::CreateTable { table } => Some((TableId::from(table), table)),
                _ => None,
            })
            .collect();

        Self {
            renames,
            added_columns,
            created_tables,
        }
    }
}

/// Apply rename decisions to a SchemaDiff, returning an updated diff
///
/// Transforms the diff by:
/// - Replacing DropTable + CreateTable with RenameTable
/// - Replacing DropColumn + AddColumn with RenameColumn  
/// - Replacing DropEnum + CreateEnum with RenameEnum
/// - Generating AlterColumn for property changes between renamed columns
pub fn apply_renames(diff: &SchemaDiff, renames: &RenameDecisions) -> SchemaDiff {
    if !renames.has_renames() {
        return diff.clone();
    }

    let ctx = RenameContext::new(diff, renames);
    let mut statements = Vec::new();

    for stmt in &diff.statements {
        match stmt {
            DiffStatement::DropTable {
                name, schema, prev, ..
            } => {
                let table_id = TableId::new(name, schema.clone());
                if let Some(new_name) = ctx.renames.get_table_new_name(&table_id) {
                    statements.extend(emit_table_rename(table_id, new_name, prev, &ctx));
                } else {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::CreateTable { table } => {
                if !ctx.renames.is_table_renamed(&TableId::from(table)) {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::DropColumn {
                table,
                schema,
                column,
                prev,
                ..
            } => {
                let table_id = TableId::new(table, schema.clone());

                // Skip if table was renamed (handled in DropTable processing)
                if ctx.renames.get_table_new_name(&table_id).is_some() {
                    continue;
                }

                if let Some(new_name) = ctx.renames.get_column_new_name(&table_id.column(column)) {
                    statements.extend(emit_column_rename(&table_id, column, new_name, prev, &ctx));
                } else {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::AddColumn {
                table,
                schema,
                column,
            } => {
                let table_id = TableId::new(table, schema.clone());

                // Skip if table was renamed or column is a rename target
                if ctx.renames.get_table_new_name(&table_id).is_none()
                    && !ctx
                        .renames
                        .is_column_renamed(&table_id.column(&column.name))
                {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::DropEnum { name, schema, .. } => {
                let enum_id = TableId::new(name, schema.clone());
                if let Some(new_name) = ctx.renames.get_enum_new_name(&enum_id) {
                    statements.push(DiffStatement::RenameEnum {
                        from: name.clone(),
                        to: new_name.to_string(),
                        schema: schema.clone(),
                    });
                } else {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::CreateEnum { name, schema, .. } => {
                if !ctx
                    .renames
                    .is_enum_renamed(&TableId::new(name, schema.clone()))
                {
                    statements.push(stmt.clone());
                }
            }

            DiffStatement::AlterColumn {
                table,
                schema,
                column,
                changes,
            } => {
                // Filter out SetType changes for renamed enums
                let filtered: Vec<_> = changes
                    .iter()
                    .filter(|c| !matches!(c, ColumnChange::SetType(t) if is_renamed_enum_type(t, ctx.renames)))
                    .cloned()
                    .collect();

                if !filtered.is_empty() {
                    statements.push(DiffStatement::AlterColumn {
                        table: table.clone(),
                        schema: schema.clone(),
                        column: column.clone(),
                        changes: filtered,
                    });
                }
            }

            _ => statements.push(stmt.clone()),
        }
    }

    SchemaDiff { statements }
}

/// Emit statements for a table rename, including column changes within that table
/// Column renames are emitted BEFORE the table rename, using the ORIGINAL table name
fn emit_table_rename(
    old_table: TableId,
    new_name: &str,
    prev: &TableSnapshot,
    ctx: &RenameContext,
) -> Vec<DiffStatement> {
    let new_table_id = old_table.with_name(new_name);

    let Some(new_table) = ctx.created_tables.get(&new_table_id) else {
        // No corresponding new table found, just emit the rename
        return vec![DiffStatement::RenameTable {
            from: old_table.name,
            to: new_name.to_string(),
            schema: old_table.schema,
        }];
    };

    let mut stmts = Vec::new();

    // Column renames FIRST (using original table name)
    for (old_col_name, old_col) in &prev.columns {
        if let Some(new_col_name) = ctx
            .renames
            .get_column_new_name(&old_table.column(old_col_name))
        {
            stmts.push(DiffStatement::RenameColumn {
                table: old_table.name.clone(),
                schema: old_table.schema.clone(),
                from: old_col_name.clone(),
                to: new_col_name.to_string(),
            });

            if let Some(new_col) = new_table.columns.get(new_col_name) {
                stmts.extend(emit_column_alterations(
                    &old_table,
                    old_col,
                    new_col_name,
                    new_col,
                    ctx.renames,
                ));
            }
        }
    }

    // Existing columns with unchanged names may still need alterations.
    for (old_col_name, old_col) in &prev.columns {
        if ctx
            .renames
            .get_column_new_name(&old_table.column(old_col_name))
            .is_some()
        {
            continue;
        }

        if let Some(new_col) = new_table.columns.get(old_col_name) {
            stmts.extend(emit_column_alterations(
                &old_table,
                old_col,
                old_col_name,
                new_col,
                ctx.renames,
            ));
        }
    }

    // Table rename AFTER column renames
    stmts.push(DiffStatement::RenameTable {
        from: old_table.name.clone(),
        to: new_name.to_string(),
        schema: old_table.schema.clone(),
    });

    // Dropped columns (not renamed, not in new table) - use new table name since rename happened
    for (old_col_name, old_col) in &prev.columns {
        if ctx
            .renames
            .get_column_new_name(&old_table.column(old_col_name))
            .is_none()
            && !new_table.columns.contains_key(old_col_name)
        {
            stmts.push(DiffStatement::DropColumn {
                table: new_table_id.name.clone(),
                schema: new_table_id.schema.clone(),
                column: old_col_name.clone(),
                cascade: false,
                prev: old_col.clone(),
            });
        }
    }

    // Added columns (not a rename target, not in old table) - use new table name
    for (new_col_name, new_col) in &new_table.columns {
        if !ctx
            .renames
            .is_column_renamed(&old_table.column(new_col_name))
            && !prev.columns.contains_key(new_col_name)
        {
            stmts.push(DiffStatement::AddColumn {
                table: new_table_id.name.clone(),
                schema: new_table_id.schema.clone(),
                column: new_col.clone(),
            });
        }
    }

    stmts
}

/// Emit statements for a column rename
fn emit_column_rename(
    table: &TableId,
    old_name: &str,
    new_name: &str,
    prev: &ColumnSnapshot,
    ctx: &RenameContext,
) -> Vec<DiffStatement> {
    let mut stmts = vec![DiffStatement::RenameColumn {
        table: table.name.clone(),
        schema: table.schema.clone(),
        from: old_name.to_string(),
        to: new_name.to_string(),
    }];

    // Add AlterColumn if properties changed
    if let Some(new_col) = ctx.added_columns.get(&table.column(new_name)) {
        stmts.extend(emit_column_alterations(
            table,
            prev,
            new_name,
            new_col,
            ctx.renames,
        ));
    }

    stmts
}

// ============================================================================
// Helper functions
// ============================================================================

fn compute_column_changes(
    from: &ColumnSnapshot,
    to: &ColumnSnapshot,
    renames: &RenameDecisions,
) -> Vec<ColumnChange> {
    let mut changes = Vec::new();

    // Type change (skip if just a renamed enum)
    if from.data_type != to.data_type
        && !is_enum_rename_type_change(&from.data_type, &to.data_type, renames)
    {
        changes.push(ColumnChange::SetType(to.data_type.clone()));
    }

    // Nullability
    match (from.nullable, to.nullable) {
        (true, false) => changes.push(ColumnChange::SetNotNull),
        (false, true) => changes.push(ColumnChange::DropNotNull),
        _ => {}
    }

    // Default
    match (&from.default, &to.default) {
        (None, Some(d)) => changes.push(ColumnChange::SetDefault(d.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropDefault),
        (Some(d1), Some(d2)) if d1 != d2 => changes.push(ColumnChange::SetDefault(d2.clone())),
        _ => {}
    }

    // Generated
    match (&from.generated, &to.generated) {
        (None, Some(g)) => changes.push(ColumnChange::SetGenerated(g.clone())),
        (Some(_), None) => changes.push(ColumnChange::DropGenerated),
        (Some(g1), Some(g2)) if g1 != g2 => changes.push(ColumnChange::SetGenerated(g2.clone())),
        _ => {}
    }

    changes
}

fn emit_column_alterations(
    table: &TableId,
    from: &ColumnSnapshot,
    to_name: &str,
    to: &ColumnSnapshot,
    renames: &RenameDecisions,
) -> Vec<DiffStatement> {
    let mut stmts = Vec::new();

    let changes = compute_column_changes(from, to, renames);
    if !changes.is_empty() {
        stmts.push(DiffStatement::AlterColumn {
            table: table.name.clone(),
            schema: table.schema.clone(),
            column: to_name.to_string(),
            changes,
        });
    }

    if from.comment != to.comment {
        stmts.push(DiffStatement::AlterColumnComment {
            table: table.name.clone(),
            schema: table.schema.clone(),
            column: to_name.to_string(),
            comment: to.comment.clone(),
            prev_comment: from.comment.clone(),
        });
    }

    stmts
}

/// Check if a type change is due to an enum being renamed
fn is_enum_rename_type_change(from: &str, to: &str, renames: &RenameDecisions) -> bool {
    let (from_base, from_suffix) = split_type_suffix(from);
    let (to_base, to_suffix) = split_type_suffix(to);

    if from_suffix != to_suffix {
        return false;
    }

    let (from_schema, from_name) = split_schema_name(from_base);
    let (to_schema, to_name) = split_schema_name(to_base);

    from_schema == to_schema
        && renames.get_enum_new_name(&TableId::new(from_name, from_schema.map(str::to_owned)))
            == Some(to_name)
}

/// Check if a type is the target of an enum rename
fn is_renamed_enum_type(type_str: &str, renames: &RenameDecisions) -> bool {
    let (base, _) = split_type_suffix(type_str);
    let (schema, name) = split_schema_name(base);
    renames.is_enum_renamed(&TableId::new(name, schema.map(str::to_owned)))
}

/// Split type into base and suffix (e.g., "status[]" -> ("status", "[]"))
fn split_type_suffix(type_str: &str) -> (&str, &str) {
    type_str
        .find('[')
        .map(|i| (&type_str[..i], &type_str[i..]))
        .unwrap_or((type_str, ""))
}

/// Split schema-qualified name (e.g., "public.status" -> (Some("public"), "status"))
fn split_schema_name(name: &str) -> (Option<&str>, &str) {
    name.rfind('.')
        .map(|i| (Some(&name[..i]), &name[i + 1..]))
        .unwrap_or((None, name))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn col(name: &str, data_type: &str) -> ColumnSnapshot {
        ColumnSnapshot {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable: true,
            default: None,
            primary_key: false,
            unique: false,
            generated: None,
            identity: None,
            comment: None,
            collation: None,
        }
    }

    fn table(name: &str, columns: Vec<(&str, &str)>) -> TableSnapshot {
        let mut cols = IndexMap::new();
        for (n, t) in columns {
            cols.insert(n.to_string(), col(n, t));
        }
        TableSnapshot {
            name: name.to_string(),
            schema: None,
            columns: cols,
            constraints: Vec::new(),
            indexes: IndexMap::new(),
            comment: None,
        }
    }

    fn tid(name: &str, schema: Option<&str>) -> TableId {
        TableId::new(name, schema.map(str::to_owned))
    }

    fn cid(table: &str, schema: Option<&str>, column: &str) -> ColumnId {
        ColumnId::new(tid(table, schema), column)
    }

    #[test]
    fn test_detect_potential_table_renames() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropTable {
                    name: "old_users".to_string(),
                    schema: None,
                    cascade: false,
                    prev: table("old_users", vec![("id", "integer")]),
                },
                DiffStatement::CreateTable {
                    table: table("new_users", vec![("id", "integer")]),
                },
            ],
        };

        let detector = RenameDetector::new(&diff);
        assert!(detector.has_potential_renames());
        assert_eq!(detector.potential_rename_count(), 1);
    }

    #[test]
    fn test_no_potential_renames_when_only_creates() {
        let diff = SchemaDiff {
            statements: vec![DiffStatement::CreateTable {
                table: table("new_users", vec![("id", "integer")]),
            }],
        };

        let detector = RenameDetector::new(&diff);
        assert!(!detector.has_potential_renames());
    }

    #[test]
    fn test_no_potential_renames_when_only_drops() {
        let diff = SchemaDiff {
            statements: vec![DiffStatement::DropTable {
                name: "old_users".to_string(),
                schema: None,
                cascade: false,
                prev: table("old_users", vec![("id", "integer")]),
            }],
        };

        let detector = RenameDetector::new(&diff);
        assert!(!detector.has_potential_renames());
    }

    #[test]
    fn test_detect_potential_column_renames() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: "old_name".to_string(),
                    cascade: false,
                    prev: col("old_name", "text"),
                },
                DiffStatement::AddColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: col("new_name", "text"),
                },
            ],
        };

        let detector = RenameDetector::new(&diff);
        assert!(detector.has_potential_renames());
        assert_eq!(detector.potential_rename_count(), 1);
    }

    #[test]
    fn test_apply_table_rename() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropTable {
                    name: "old_users".to_string(),
                    schema: None,
                    cascade: false,
                    prev: table("old_users", vec![("id", "integer")]),
                },
                DiffStatement::CreateTable {
                    table: table("new_users", vec![("id", "integer")]),
                },
            ],
        };

        let mut renames = RenameDecisions::new();
        renames.insert_table_decision(
            &tid("old_users", None),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_users".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        assert_eq!(result.statements.len(), 1);
        assert!(matches!(
            &result.statements[0],
            DiffStatement::RenameTable { from, to, .. }
            if from == "old_users" && to == "new_users"
        ));
    }

    #[test]
    fn test_apply_column_rename() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: "old_name".to_string(),
                    cascade: false,
                    prev: col("old_name", "text"),
                },
                DiffStatement::AddColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: col("new_name", "text"),
                },
            ],
        };

        let mut renames = RenameDecisions::new();
        renames.insert_column_decision(
            &cid("users", None, "old_name"),
            RenameDecision::Rename {
                from: "old_name".to_string(),
                to: "new_name".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        assert_eq!(result.statements.len(), 1);
        assert!(matches!(
            &result.statements[0],
            DiffStatement::RenameColumn { table, from, to, .. }
            if table == "users" && from == "old_name" && to == "new_name"
        ));
    }

    #[test]
    fn test_apply_enum_rename() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropEnum {
                    name: "old_status".to_string(),
                    schema: None,
                    prev: EnumSnapshot {
                        name: "old_status".to_string(),
                        schema: None,
                        values: vec!["active".to_string(), "inactive".to_string()],
                        description: None,
                    },
                },
                DiffStatement::CreateEnum {
                    name: "new_status".to_string(),
                    schema: None,
                    values: vec!["active".to_string(), "inactive".to_string()],
                    description: None,
                },
            ],
        };

        let mut renames = RenameDecisions::new();
        renames.insert_enum_decision(
            &tid("old_status", None),
            RenameDecision::Rename {
                from: "old_status".to_string(),
                to: "new_status".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        assert_eq!(result.statements.len(), 1);
        assert!(matches!(
            &result.statements[0],
            DiffStatement::RenameEnum { from, to, .. }
            if from == "old_status" && to == "new_status"
        ));
    }

    #[test]
    fn test_no_renames_keeps_diff_unchanged() {
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropTable {
                    name: "old_users".to_string(),
                    schema: None,
                    cascade: false,
                    prev: table("old_users", vec![("id", "integer")]),
                },
                DiffStatement::CreateTable {
                    table: table("new_users", vec![("id", "integer")]),
                },
            ],
        };

        let renames = RenameDecisions::new();
        let result = apply_renames(&diff, &renames);

        assert_eq!(result.statements.len(), 2);
    }

    #[test]
    fn test_rename_decisions_basic() {
        let mut decisions = RenameDecisions::new();

        decisions.insert_column_decision(
            &cid("users", None, "old_col"),
            RenameDecision::Rename {
                from: "old_col".to_string(),
                to: "new_col".to_string(),
            },
        );

        assert_eq!(
            decisions.get_column_new_name(&cid("users", None, "old_col")),
            Some("new_col")
        );
        assert!(decisions.is_column_renamed(&cid("users", None, "new_col")));
        assert!(!decisions.is_column_renamed(&cid("users", None, "other_col")));
    }

    #[test]
    fn test_get_original_table_name() {
        let mut decisions = RenameDecisions::new();
        decisions.insert_table_decision(
            &tid("old_users", None),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_accounts".to_string(),
            },
        );

        assert_eq!(
            decisions.get_table_old_name(&tid("new_accounts", None)),
            Some("old_users")
        );
        assert_eq!(
            decisions.get_table_old_name(&tid("other_table", None)),
            None
        );
    }

    #[test]
    fn test_column_rename_in_renamed_table() {
        // Scenario: table "old_users" -> "new_users" AND column "old_name" -> "new_name"
        // Column renames should use ORIGINAL table name and come BEFORE table rename
        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropTable {
                    name: "old_users".to_string(),
                    schema: None,
                    cascade: false,
                    prev: table("old_users", vec![("id", "integer"), ("old_name", "text")]),
                },
                DiffStatement::CreateTable {
                    table: table("new_users", vec![("id", "integer"), ("new_name", "text")]),
                },
            ],
        };

        let mut renames = RenameDecisions::new();
        renames.insert_table_decision(
            &tid("old_users", None),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_users".to_string(),
            },
        );
        // Column renames keyed by ORIGINAL table name
        renames.insert_column_decision(
            &cid("old_users", None, "old_name"),
            RenameDecision::Rename {
                from: "old_name".to_string(),
                to: "new_name".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        // Should have: RenameColumn (using original table) + RenameTable
        assert_eq!(result.statements.len(), 2);

        let has_table_rename = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameTable { from, to, .. }
                if from == "old_users" && to == "new_users")
        });
        // Column renames use ORIGINAL table name
        let has_column_rename = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameColumn { table, from, to, .. }
                if table == "old_users" && from == "old_name" && to == "new_name")
        });

        assert!(has_table_rename, "Expected RenameTable statement");
        assert!(
            has_column_rename,
            "Expected RenameColumn statement with original table name"
        );
    }

    #[test]
    fn test_table_rename_keeps_same_name_column_alters() {
        let mut old_price = col("price", "integer");
        old_price.comment = Some("old comment".to_string());

        let mut new_price = col("price", "bigint");
        new_price.comment = Some("new comment".to_string());

        let mut old_columns = IndexMap::new();
        old_columns.insert("price".to_string(), old_price);

        let mut new_columns = IndexMap::new();
        new_columns.insert("price".to_string(), new_price);

        let mut diff = SchemaDiff { statements: vec![] };
        diff.statements.push(DiffStatement::DropTable {
            name: "orders_old".to_string(),
            schema: None,
            cascade: false,
            prev: TableSnapshot {
                name: "orders_old".to_string(),
                schema: None,
                columns: old_columns,
                constraints: Vec::new(),
                indexes: IndexMap::new(),
                comment: None,
            },
        });
        diff.statements.push(DiffStatement::CreateTable {
            table: TableSnapshot {
                name: "orders".to_string(),
                schema: None,
                columns: new_columns,
                constraints: Vec::new(),
                indexes: IndexMap::new(),
                comment: None,
            },
        });

        let mut renames = RenameDecisions::new();
        renames.insert_table_decision(
            &tid("orders_old", None),
            RenameDecision::Rename {
                from: "orders_old".to_string(),
                to: "orders".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        let has_table_rename = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::RenameTable { from, to, .. } if from == "orders_old" && to == "orders")
        });
        let has_column_alter = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumn { table, column, changes, .. }
                if table == "orders_old" && column == "price"
                && changes.iter().any(|c| matches!(c, ColumnChange::SetType(t) if t == "bigint")))
        });
        let has_comment_alter = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumnComment { table, column, comment, .. }
                if table == "orders_old" && column == "price" && comment.as_deref() == Some("new comment"))
        });

        assert!(has_table_rename, "Expected RenameTable statement");
        assert!(
            has_column_alter,
            "Expected AlterColumn for same-name column in renamed table"
        );
        assert!(
            has_comment_alter,
            "Expected AlterColumnComment for same-name column in renamed table"
        );
    }

    #[test]
    fn test_column_rename_emits_comment_alter() {
        let mut old_col = col("full_name", "text");
        old_col.comment = Some("legacy".to_string());

        let mut new_col = col("name", "text");
        new_col.comment = Some("canonical".to_string());

        let diff = SchemaDiff {
            statements: vec![
                DiffStatement::DropColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: "full_name".to_string(),
                    cascade: false,
                    prev: old_col,
                },
                DiffStatement::AddColumn {
                    table: "users".to_string(),
                    schema: None,
                    column: new_col,
                },
            ],
        };

        let mut renames = RenameDecisions::new();
        renames.insert_column_decision(
            &cid("users", None, "full_name"),
            RenameDecision::Rename {
                from: "full_name".to_string(),
                to: "name".to_string(),
            },
        );

        let result = apply_renames(&diff, &renames);

        let has_comment_alter = result.statements.iter().any(|s| {
            matches!(s, DiffStatement::AlterColumnComment { table, column, comment, .. }
                if table == "users" && column == "name" && comment.as_deref() == Some("canonical"))
        });

        assert!(
            has_comment_alter,
            "Expected AlterColumnComment to be emitted after rename"
        );
    }

    #[test]
    fn test_schema_scoped_table_rename_lookup() {
        let mut decisions = RenameDecisions::new();
        decisions.insert_table_decision(
            &tid("users", Some("sales")),
            RenameDecision::Rename {
                from: "users".to_string(),
                to: "accounts".to_string(),
            },
        );

        assert_eq!(
            decisions.get_table_new_name(&tid("users", Some("sales"))),
            Some("accounts")
        );
        assert_eq!(
            decisions.get_table_new_name(&tid("users", Some("public"))),
            None
        );
    }

    #[test]
    fn test_schema_scoped_enum_rename_lookup() {
        let mut decisions = RenameDecisions::new();
        decisions.insert_enum_decision(
            &tid("status", Some("billing")),
            RenameDecision::Rename {
                from: "status".to_string(),
                to: "invoice_status".to_string(),
            },
        );

        assert_eq!(
            decisions.get_enum_new_name(&tid("status", Some("billing"))),
            Some("invoice_status")
        );
        assert_eq!(decisions.get_enum_new_name(&tid("status", None)), None);
    }
}
