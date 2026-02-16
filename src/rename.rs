//! Rename detection for schema diffs
//!
//! This module detects potential renames when columns, tables, or enums are
//! added and dropped in the same diff. It prompts the user to decide whether
//! these should be treated as renames (preserving data) or actual add/drop
//! operations.
//!
//! The detection happens in phases to properly handle cross-entity relationships:
//! 1. First, table and enum renames are detected and prompted
//! 2. Then, column renames are detected considering table rename decisions
//!    (e.g., columns in a renamed table are properly tracked)

use colored::Colorize;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use indexmap::IndexMap;

use crate::snapshot::{ColumnSnapshot, EnumSnapshot, Snapshot, TableSnapshot};
use crate::{Result, ShkiError};

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
    /// Table renames: key is the dropped table name (in original schema)
    pub tables: IndexMap<String, RenameDecision>,
    /// Column renames: key is (original_table_name, dropped_column_name)
    /// Note: original_table_name is the name in the 'from' snapshot, even if the table was renamed
    pub columns: IndexMap<(String, String), RenameDecision>,
    /// Enum renames: key is the dropped enum name
    pub enums: IndexMap<String, RenameDecision>,
}

impl RenameDecisions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a table should be renamed instead of dropped
    pub fn get_table_rename(&self, dropped_name: &str) -> Option<&str> {
        match self.tables.get(dropped_name) {
            Some(RenameDecision::Rename { to, .. }) => Some(to.as_str()),
            _ => None,
        }
    }

    /// Get the original table name for a given new table name (if it was renamed)
    pub fn get_original_table_name(&self, new_name: &str) -> Option<&str> {
        for (old_name, decision) in &self.tables {
            if let RenameDecision::Rename { to, .. } = decision
                && to == new_name
            {
                return Some(old_name.as_str());
            }
        }
        None
    }

    /// Check if a column should be renamed instead of dropped
    /// The table parameter should be the ORIGINAL table name (from the 'from' snapshot)
    pub fn get_column_rename(&self, original_table: &str, dropped_column: &str) -> Option<&str> {
        match self
            .columns
            .get(&(original_table.to_string(), dropped_column.to_string()))
        {
            Some(RenameDecision::Rename { to, .. }) => Some(to.as_str()),
            _ => None,
        }
    }

    /// Check if an enum should be renamed instead of dropped
    pub fn get_enum_rename(&self, dropped_name: &str) -> Option<&str> {
        match self.enums.get(dropped_name) {
            Some(RenameDecision::Rename { to, .. }) => Some(to.as_str()),
            _ => None,
        }
    }

    /// Check if a table is the target of a rename (and should not be created)
    pub fn is_table_rename_target(&self, added_name: &str) -> bool {
        self.tables
            .values()
            .any(|d| matches!(d, RenameDecision::Rename { to, .. } if to == added_name))
    }

    /// Check if a column is the target of a rename (and should not be added)
    /// The table parameter should be the ORIGINAL table name
    pub fn is_column_rename_target(&self, original_table: &str, added_column: &str) -> bool {
        self.columns.iter().any(|((t, _), d)| {
            t == original_table
                && matches!(d, RenameDecision::Rename { to, .. } if to == added_column)
        })
    }

    /// Check if an enum is the target of a rename (and should not be created)
    pub fn is_enum_rename_target(&self, added_name: &str) -> bool {
        self.enums
            .values()
            .any(|d| matches!(d, RenameDecision::Rename { to, .. } if to == added_name))
    }
}

/// Information about a potential column rename, including table context
#[derive(Debug, Clone)]
struct ColumnRenameContext {
    /// Original table name (in 'from' snapshot)
    original_table_name: String,
    /// New table name (in 'to' snapshot) - may be same as original or different if table was renamed
    new_table_name: String,
    /// Schema of the table
    schema: Option<String>,
    /// Columns that were dropped from this table
    dropped_columns: Vec<ColumnSnapshot>,
    /// Columns that were added to this table
    added_columns: Vec<ColumnSnapshot>,
}

/// Detects potential renames between two snapshots
///
/// This detector works in phases:
/// 1. Detect table and enum renames first (structural changes)
/// 2. After those decisions are made, detect column renames considering table renames
pub struct RenameDetector<'a> {
    from: &'a Snapshot,
    to: &'a Snapshot,
}

impl<'a> RenameDetector<'a> {
    /// Create a new rename detector
    pub fn new(from: &'a Snapshot, to: &'a Snapshot) -> Self {
        Self { from, to }
    }

    /// Check if there are any potential renames to prompt for
    pub fn has_potential_renames(&self) -> bool {
        let (dropped_tables, added_tables) = self.get_table_changes();
        let (dropped_enums, added_enums) = self.get_enum_changes();

        // Tables: must have both adds and drops
        let has_table_renames = !dropped_tables.is_empty() && !added_tables.is_empty();

        // Enums: must have both adds and drops
        let has_enum_renames = !dropped_enums.is_empty() && !added_enums.is_empty();

        // For columns, we need to check tables that exist in both (same name)
        // OR tables where one was dropped and one was added (potential rename)
        let has_column_renames = self.has_potential_column_renames(&dropped_tables, &added_tables);

        has_table_renames || has_enum_renames || has_column_renames
    }

    /// Get count of potential rename scenarios
    pub fn potential_rename_count(&self) -> usize {
        let (dropped_tables, added_tables) = self.get_table_changes();
        let (dropped_enums, added_enums) = self.get_enum_changes();

        let table_count = if !dropped_tables.is_empty() && !added_tables.is_empty() {
            dropped_tables.len()
        } else {
            0
        };

        let enum_count = if !dropped_enums.is_empty() && !added_enums.is_empty() {
            dropped_enums.len()
        } else {
            0
        };

        // Column count is harder to determine without table decisions
        // For now, estimate based on tables that exist in both snapshots
        let column_count: usize = self
            .from
            .tables
            .iter()
            .filter_map(|(name, from_table)| {
                self.to.tables.get(name).map(|to_table| {
                    let dropped: Vec<_> = from_table
                        .columns
                        .keys()
                        .filter(|c| !to_table.columns.contains_key(*c))
                        .collect();
                    let added: Vec<_> = to_table
                        .columns
                        .keys()
                        .filter(|c| !from_table.columns.contains_key(*c))
                        .collect();
                    if !dropped.is_empty() && !added.is_empty() {
                        dropped.len()
                    } else {
                        0
                    }
                })
            })
            .sum();

        table_count + enum_count + column_count
    }

    /// Prompt the user for rename decisions in the correct order
    ///
    /// Returns the user's decisions for all potential renames.
    /// If running non-interactively, returns default decisions (keep add/drop).
    pub fn prompt_for_decisions(&self, interactive: bool) -> Result<RenameDecisions> {
        let mut decisions = RenameDecisions::new();

        if !interactive {
            return Ok(decisions);
        }

        // Phase 1: Prompt for table renames first
        let (dropped_tables, added_tables) = self.get_table_changes();
        if !dropped_tables.is_empty() && !added_tables.is_empty() {
            println!("\n{}", "Detected potential table renames:".cyan().bold());
            println!(
                "{}",
                "  (Renaming preserves table data, dropping loses all data)".dimmed()
            );
            for dropped in &dropped_tables {
                let decision =
                    self.prompt_table_rename(dropped, &added_tables, &decisions.tables)?;
                decisions.tables.insert(dropped.name.clone(), decision);
            }
        }

        // Phase 2: Prompt for enum renames
        let (dropped_enums, added_enums) = self.get_enum_changes();
        if !dropped_enums.is_empty() && !added_enums.is_empty() {
            println!("\n{}", "Detected potential enum renames:".cyan().bold());
            for dropped in &dropped_enums {
                let decision = self.prompt_enum_rename(dropped, &added_enums, &decisions.enums)?;
                decisions.enums.insert(dropped.name.clone(), decision);
            }
        }

        // Phase 3: Detect and prompt for column renames, now that we know about table renames
        let column_contexts = self.get_column_rename_contexts(&decisions);
        for ctx in column_contexts {
            if !ctx.dropped_columns.is_empty() && !ctx.added_columns.is_empty() {
                let display_name = if ctx.original_table_name != ctx.new_table_name {
                    format!(
                        "{} (renamed to {})",
                        format_table_name(&ctx.original_table_name, &ctx.schema).yellow(),
                        ctx.new_table_name.green()
                    )
                } else {
                    format_table_name(&ctx.original_table_name, &ctx.schema)
                        .yellow()
                        .to_string()
                };

                println!(
                    "\n{} {}",
                    "Detected potential column renames in table".cyan().bold(),
                    display_name
                );
                println!(
                    "{}",
                    "  (Renaming columns preserves data, while drop+add loses data)".dimmed()
                );

                for dropped in &ctx.dropped_columns {
                    // Filter out columns that have already been chosen as rename targets
                    let available_added: Vec<_> = ctx
                        .added_columns
                        .iter()
                        .filter(|added| {
                            !decisions
                                .is_column_rename_target(&ctx.original_table_name, &added.name)
                        })
                        .cloned()
                        .collect();

                    if available_added.is_empty() {
                        // No available targets, must drop
                        decisions.columns.insert(
                            (ctx.original_table_name.clone(), dropped.name.clone()),
                            RenameDecision::KeepAddDrop,
                        );
                        continue;
                    }

                    let decision = self.prompt_column_rename(
                        &ctx.original_table_name,
                        &ctx.new_table_name,
                        &ctx.schema,
                        dropped,
                        &available_added,
                    )?;
                    decisions.columns.insert(
                        (ctx.original_table_name.clone(), dropped.name.clone()),
                        decision,
                    );
                }
            }
        }

        Ok(decisions)
    }

    /// Get tables that were dropped and added
    fn get_table_changes(&self) -> (Vec<TableSnapshot>, Vec<TableSnapshot>) {
        let dropped: Vec<_> = self
            .from
            .tables
            .values()
            .filter(|t| !self.to.tables.contains_key(&t.name))
            .cloned()
            .collect();

        let added: Vec<_> = self
            .to
            .tables
            .values()
            .filter(|t| !self.from.tables.contains_key(&t.name))
            .cloned()
            .collect();

        (dropped, added)
    }

    /// Get enums that were dropped and added
    fn get_enum_changes(&self) -> (Vec<EnumSnapshot>, Vec<EnumSnapshot>) {
        let dropped: Vec<_> = self
            .from
            .enums
            .values()
            .filter(|e| !self.to.enums.contains_key(&e.name))
            .cloned()
            .collect();

        let added: Vec<_> = self
            .to
            .enums
            .values()
            .filter(|e| !self.from.enums.contains_key(&e.name))
            .cloned()
            .collect();

        (dropped, added)
    }

    /// Check if there are potential column renames
    fn has_potential_column_renames(
        &self,
        dropped_tables: &[TableSnapshot],
        added_tables: &[TableSnapshot],
    ) -> bool {
        // Check tables that exist with the same name in both snapshots
        for (name, from_table) in &self.from.tables {
            if let Some(to_table) = self.to.tables.get(name) {
                let has_dropped = from_table
                    .columns
                    .keys()
                    .any(|c| !to_table.columns.contains_key(c));
                let has_added = to_table
                    .columns
                    .keys()
                    .any(|c| !from_table.columns.contains_key(c));
                if has_dropped && has_added {
                    return true;
                }
            }
        }

        // Check tables that might be renamed (dropped table has columns, added table has columns)
        // This is a potential rename scenario
        for dropped in dropped_tables {
            for added in added_tables {
                // If both tables have columns, there might be column renames within the renamed table
                if !dropped.columns.is_empty() && !added.columns.is_empty() {
                    let has_dropped_cols = dropped
                        .columns
                        .keys()
                        .any(|c| !added.columns.contains_key(c));
                    let has_added_cols = added
                        .columns
                        .keys()
                        .any(|c| !dropped.columns.contains_key(c));
                    if has_dropped_cols && has_added_cols {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Get column rename contexts after table rename decisions are made
    fn get_column_rename_contexts(&self, decisions: &RenameDecisions) -> Vec<ColumnRenameContext> {
        let mut contexts = Vec::new();

        // Case 1: Tables that exist with the same name in both snapshots
        for (name, from_table) in &self.from.tables {
            if let Some(to_table) = self.to.tables.get(name) {
                let dropped: Vec<_> = from_table
                    .columns
                    .values()
                    .filter(|c| !to_table.columns.contains_key(&c.name))
                    .cloned()
                    .collect();

                let added: Vec<_> = to_table
                    .columns
                    .values()
                    .filter(|c| !from_table.columns.contains_key(&c.name))
                    .cloned()
                    .collect();

                if !dropped.is_empty() || !added.is_empty() {
                    contexts.push(ColumnRenameContext {
                        original_table_name: name.clone(),
                        new_table_name: name.clone(),
                        schema: from_table.schema.clone(),
                        dropped_columns: dropped,
                        added_columns: added,
                    });
                }
            }
        }

        // Case 2: Tables that were renamed - use the user's decision to pair them
        for (old_name, decision) in &decisions.tables {
            if let RenameDecision::Rename { to: new_name, .. } = decision
                && let (Some(from_table), Some(to_table)) =
                    (self.from.tables.get(old_name), self.to.tables.get(new_name))
            {
                // Find column changes between the old and new table
                let dropped: Vec<_> = from_table
                    .columns
                    .values()
                    .filter(|c| !to_table.columns.contains_key(&c.name))
                    .cloned()
                    .collect();

                let added: Vec<_> = to_table
                    .columns
                    .values()
                    .filter(|c| !from_table.columns.contains_key(&c.name))
                    .cloned()
                    .collect();

                if !dropped.is_empty() || !added.is_empty() {
                    contexts.push(ColumnRenameContext {
                        original_table_name: old_name.clone(),
                        new_table_name: new_name.clone(),
                        schema: from_table.schema.clone(),
                        dropped_columns: dropped,
                        added_columns: added,
                    });
                }
            }
        }

        contexts
    }

    fn prompt_table_rename(
        &self,
        dropped: &TableSnapshot,
        added_tables: &[TableSnapshot],
        existing_decisions: &IndexMap<String, RenameDecision>,
    ) -> Result<RenameDecision> {
        let dropped_name = format_table_name(&dropped.name, &dropped.schema);

        // Filter out tables that have already been chosen as rename targets
        let available_targets: Vec<_> = added_tables
            .iter()
            .filter(|added| {
                !existing_decisions
                    .values()
                    .any(|d| matches!(d, RenameDecision::Rename { to, .. } if to == &added.name))
            })
            .collect();

        // Build options
        let mut options = vec![format!(
            "{} table {} (all data will be lost)",
            "Drop".red(),
            dropped_name.yellow()
        )];

        for added in &available_targets {
            let added_name = format_table_name(&added.name, &added.schema);
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

        if available_targets.is_empty() {
            // No available rename targets, must drop
            println!(
                "  {} {} (no available rename targets)",
                "Dropping".red(),
                dropped_name.yellow()
            );
            return Ok(RenameDecision::KeepAddDrop);
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

        if selection == 0 {
            Ok(RenameDecision::KeepAddDrop)
        } else {
            let target = available_targets[selection - 1];
            Ok(RenameDecision::Rename {
                from: dropped.name.clone(),
                to: target.name.clone(),
            })
        }
    }

    fn prompt_column_rename(
        &self,
        original_table_name: &str,
        new_table_name: &str,
        _schema: &Option<String>,
        dropped: &ColumnSnapshot,
        added_columns: &[ColumnSnapshot],
    ) -> Result<RenameDecision> {
        // Display table name (show both if renamed)
        let table_display = if original_table_name != new_table_name {
            format!(
                "{}->{}",
                original_table_name.dimmed(),
                new_table_name.cyan()
            )
        } else {
            new_table_name.cyan().to_string()
        };

        // Build options
        let mut options = vec![format!(
            "{} column {} ({}) - data will be lost",
            "Drop".red(),
            dropped.name.yellow(),
            dropped.data_type.dimmed()
        )];

        for added in added_columns {
            let type_match = if dropped.data_type == added.data_type {
                "(same type)".green()
            } else {
                format!(
                    "(type changes: {} -> {})",
                    dropped.data_type, added.data_type
                )
                .yellow()
            };

            options.push(format!(
                "{} {} {} {} {}",
                "Rename".cyan(),
                dropped.name.yellow(),
                "to".dimmed(),
                added.name.green(),
                type_match
            ));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Column {}.{} was dropped. What should happen?",
                table_display,
                dropped.name.yellow()
            ))
            .default(0)
            .items(&options)
            .interact_opt()
            .map_err(|e| ShkiError::config(format!("Prompt error: {}", e)))?
            .ok_or(ShkiError::Cancelled)?;

        if selection == 0 {
            Ok(RenameDecision::KeepAddDrop)
        } else {
            let target = &added_columns[selection - 1];
            Ok(RenameDecision::Rename {
                from: dropped.name.clone(),
                to: target.name.clone(),
            })
        }
    }

    fn prompt_enum_rename(
        &self,
        dropped: &EnumSnapshot,
        added_enums: &[EnumSnapshot],
        existing_decisions: &IndexMap<String, RenameDecision>,
    ) -> Result<RenameDecision> {
        let dropped_name = format_enum_name(&dropped.name, &dropped.schema);

        // Filter out enums that have already been chosen as rename targets
        let available_targets: Vec<_> = added_enums
            .iter()
            .filter(|added| {
                !existing_decisions
                    .values()
                    .any(|d| matches!(d, RenameDecision::Rename { to, .. } if to == &added.name))
            })
            .collect();

        // Build options
        let mut options = vec![format!("{} enum {}", "Drop".red(), dropped_name.yellow())];

        for added in &available_targets {
            let added_name = format_enum_name(&added.name, &added.schema);
            let values_info = if dropped.values == added.values {
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
                values_info
            ));
        }

        if available_targets.is_empty() {
            // No available rename targets, must drop
            println!(
                "  {} {} (no available rename targets)",
                "Dropping".red(),
                dropped_name.yellow()
            );
            return Ok(RenameDecision::KeepAddDrop);
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

        if selection == 0 {
            Ok(RenameDecision::KeepAddDrop)
        } else {
            let target = available_targets[selection - 1];
            Ok(RenameDecision::Rename {
                from: dropped.name.clone(),
                to: target.name.clone(),
            })
        }
    }
}

fn format_table_name(name: &str, schema: &Option<String>) -> String {
    match schema {
        Some(s) => format!("{}.{}", s, name),
        None => name.to_string(),
    }
}

fn format_enum_name(name: &str, schema: &Option<String>) -> String {
    match schema {
        Some(s) => format!("{}.{}", s, name),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaDialect;

    fn create_test_column(name: &str, data_type: &str) -> ColumnSnapshot {
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

    fn create_test_table(name: &str, columns: Vec<(&str, &str)>) -> TableSnapshot {
        let mut cols = IndexMap::new();
        for (col_name, col_type) in columns {
            cols.insert(col_name.to_string(), create_test_column(col_name, col_type));
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

    #[test]
    fn test_detect_column_rename_in_same_table() {
        let mut from = Snapshot::new(SchemaDialect::Postgres);
        from.tables.insert(
            "users".to_string(),
            create_test_table("users", vec![("id", "integer"), ("old_name", "text")]),
        );

        let mut to = Snapshot::new(SchemaDialect::Postgres);
        to.tables.insert(
            "users".to_string(),
            create_test_table("users", vec![("id", "integer"), ("new_name", "text")]),
        );

        let detector = RenameDetector::new(&from, &to);

        assert!(detector.has_potential_renames());
    }

    #[test]
    fn test_detect_table_rename_candidates() {
        let mut from = Snapshot::new(SchemaDialect::Postgres);
        from.tables.insert(
            "old_table".to_string(),
            create_test_table("old_table", vec![("id", "integer")]),
        );

        let mut to = Snapshot::new(SchemaDialect::Postgres);
        to.tables.insert(
            "new_table".to_string(),
            create_test_table("new_table", vec![("id", "integer")]),
        );

        let detector = RenameDetector::new(&from, &to);

        assert!(detector.has_potential_renames());
        let (dropped, added) = detector.get_table_changes();
        assert_eq!(dropped.len(), 1);
        assert_eq!(added.len(), 1);
    }

    #[test]
    fn test_no_rename_candidates_when_only_adds() {
        let from = Snapshot::new(SchemaDialect::Postgres);

        let mut to = Snapshot::new(SchemaDialect::Postgres);
        to.tables.insert(
            "new_table".to_string(),
            create_test_table("new_table", vec![("id", "integer")]),
        );

        let detector = RenameDetector::new(&from, &to);

        assert!(!detector.has_potential_renames());
    }

    #[test]
    fn test_rename_decisions_basic() {
        let mut decisions = RenameDecisions::new();

        // Add a column rename decision
        decisions.columns.insert(
            ("users".to_string(), "old_col".to_string()),
            RenameDecision::Rename {
                from: "old_col".to_string(),
                to: "new_col".to_string(),
            },
        );

        assert_eq!(
            decisions.get_column_rename("users", "old_col"),
            Some("new_col")
        );
        assert!(decisions.is_column_rename_target("users", "new_col"));
        assert!(!decisions.is_column_rename_target("users", "other_col"));
    }

    #[test]
    fn test_get_original_table_name() {
        let mut decisions = RenameDecisions::new();
        decisions.tables.insert(
            "old_users".to_string(),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_accounts".to_string(),
            },
        );

        assert_eq!(
            decisions.get_original_table_name("new_accounts"),
            Some("old_users")
        );
        assert_eq!(decisions.get_original_table_name("other_table"), None);
    }

    #[test]
    fn test_column_rename_contexts_with_table_rename() {
        let mut from = Snapshot::new(SchemaDialect::Postgres);
        from.tables.insert(
            "old_users".to_string(),
            create_test_table("old_users", vec![("id", "integer"), ("old_col", "text")]),
        );

        let mut to = Snapshot::new(SchemaDialect::Postgres);
        to.tables.insert(
            "new_accounts".to_string(),
            create_test_table("new_accounts", vec![("id", "integer"), ("new_col", "text")]),
        );

        let detector = RenameDetector::new(&from, &to);

        // First, simulate a table rename decision
        let mut decisions = RenameDecisions::new();
        decisions.tables.insert(
            "old_users".to_string(),
            RenameDecision::Rename {
                from: "old_users".to_string(),
                to: "new_accounts".to_string(),
            },
        );

        // Now get column rename contexts
        let contexts = detector.get_column_rename_contexts(&decisions);

        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].original_table_name, "old_users");
        assert_eq!(contexts[0].new_table_name, "new_accounts");
        assert_eq!(contexts[0].dropped_columns.len(), 1);
        assert_eq!(contexts[0].dropped_columns[0].name, "old_col");
        assert_eq!(contexts[0].added_columns.len(), 1);
        assert_eq!(contexts[0].added_columns[0].name, "new_col");
    }

    #[test]
    fn test_has_potential_column_renames_in_renamed_table() {
        let mut from = Snapshot::new(SchemaDialect::Postgres);
        from.tables.insert(
            "old_users".to_string(),
            create_test_table("old_users", vec![("id", "integer"), ("old_col", "text")]),
        );

        let mut to = Snapshot::new(SchemaDialect::Postgres);
        to.tables.insert(
            "new_accounts".to_string(),
            create_test_table("new_accounts", vec![("id", "integer"), ("new_col", "text")]),
        );

        let detector = RenameDetector::new(&from, &to);
        let (dropped_tables, added_tables) = detector.get_table_changes();

        // Should detect potential column renames even when tables themselves might be renamed
        assert!(detector.has_potential_column_renames(&dropped_tables, &added_tables));
    }
}
