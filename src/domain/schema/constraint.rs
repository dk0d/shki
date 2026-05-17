//! Constraint definitions

use serde::{Deserialize, Serialize};

use crate::models::entity_name::EntityName;

use super::types::ReferenceAction;

pub enum ConstraintType {
    PrimaryKey,
    Unique,
    ForeignKey,
    Check,
    Exclusion,
}

/// A table constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Constraint {
    /// Primary key constraint
    PrimaryKey(PrimaryKeyConstraint),
    /// Unique constraint
    Unique(UniqueConstraint),
    /// Foreign key constraint
    ForeignKey(ForeignKeyConstraint),
    /// Check constraint
    Check(CheckConstraint),
    /// Exclusion constraint (PostgreSQL)
    Exclusion(ExclusionConstraint),
}

impl Constraint {
    /// Get the constraint name
    pub fn name(&self) -> Option<&str> {
        match self {
            Constraint::PrimaryKey(c) => c.name.as_deref(),
            Constraint::Unique(c) => c.name.as_deref(),
            Constraint::ForeignKey(c) => c.name.as_deref(),
            Constraint::Check(c) => c.name.as_deref(),
            Constraint::Exclusion(c) => c.name.as_deref(),
        }
    }
}

/// Primary key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryKeyConstraint {
    /// Constraint name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Columns that form the primary key
    pub columns: Vec<String>,
}

impl PrimaryKeyConstraint {
    /// Create a new primary key constraint
    pub fn new(columns: Vec<impl Into<String>>) -> Self {
        Self {
            name: None,
            columns: columns.into_iter().map(Into::into).collect(),
        }
    }

    /// Set the constraint name
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Unique constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniqueConstraint {
    /// Constraint name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Columns that form the unique constraint
    pub columns: Vec<String>,
    /// Nulls distinct (PostgreSQL 15+)
    #[serde(default = "default_true")]
    pub nulls_distinct: bool,
}

fn default_true() -> bool {
    true
}

impl UniqueConstraint {
    /// Create a new unique constraint
    pub fn new(columns: Vec<impl Into<String>>) -> Self {
        Self {
            name: None,
            columns: columns.into_iter().map(Into::into).collect(),
            nulls_distinct: true,
        }
    }

    /// Set the constraint name
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set nulls not distinct (PostgreSQL 15+)
    pub fn nulls_not_distinct(mut self) -> Self {
        self.nulls_distinct = false;
        self
    }
}

/// Foreign key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyConstraint {
    /// Constraint name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Local columns
    pub columns: Vec<String>,
    /// Referenced table
    pub references: EntityName,
    /// Referenced columns
    pub references_columns: Vec<String>,
    /// ON DELETE action
    #[serde(default)]
    pub on_delete: ReferenceAction,
    /// ON UPDATE action
    #[serde(default)]
    pub on_update: ReferenceAction,
    /// Deferrable constraint
    #[serde(default)]
    pub deferrable: bool,
    /// Initially deferred
    #[serde(default)]
    pub initially_deferred: bool,
}

impl ForeignKeyConstraint {
    /// Create a new foreign key constraint
    pub fn new(
        columns: Vec<impl Into<String>>,
        references_table: impl Into<EntityName>,
        references_columns: Vec<impl Into<String>>,
    ) -> Self {
        Self {
            name: None,
            columns: columns.into_iter().map(Into::into).collect(),
            references: references_table.into(),
            references_columns: references_columns.into_iter().map(Into::into).collect(),
            on_delete: ReferenceAction::NoAction,
            on_update: ReferenceAction::NoAction,
            deferrable: false,
            initially_deferred: false,
        }
    }

    /// Set the constraint name
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set ON DELETE action
    pub fn on_delete(mut self, action: ReferenceAction) -> Self {
        self.on_delete = action;
        self
    }

    /// Set ON UPDATE action
    pub fn on_update(mut self, action: ReferenceAction) -> Self {
        self.on_update = action;
        self
    }

    /// Make the constraint deferrable
    pub fn deferrable(mut self) -> Self {
        self.deferrable = true;
        self
    }

    /// Make the constraint initially deferred
    pub fn initially_deferred(mut self) -> Self {
        self.initially_deferred = true;
        self.deferrable = true;
        self
    }
}

/// Check constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConstraint {
    /// Constraint name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Check expression
    pub expression: String,
}

impl CheckConstraint {
    /// Create a new check constraint
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            name: None,
            expression: expression.into(),
        }
    }

    /// Set the constraint name
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Exclusion constraint (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusionConstraint {
    /// Constraint name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Index method (e.g., "gist", "spgist")
    #[serde(default = "default_gist")]
    pub using: String,
    /// Exclusion elements: (column/expression, operator)
    pub elements: Vec<(String, String)>,
    /// WHERE clause
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,
}

fn default_gist() -> String {
    "gist".to_string()
}

impl ExclusionConstraint {
    /// Create a new exclusion constraint
    pub fn new(elements: Vec<(impl Into<String>, impl Into<String>)>) -> Self {
        Self {
            name: None,
            using: "gist".to_string(),
            elements: elements
                .into_iter()
                .map(|(e, o)| (e.into(), o.into()))
                .collect(),
            where_clause: None,
        }
    }

    /// Set the constraint name
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the index method
    pub fn using(mut self, method: impl Into<String>) -> Self {
        self.using = method.into();
        self
    }

    /// Set the WHERE clause
    pub fn where_clause(mut self, clause: impl Into<String>) -> Self {
        self.where_clause = Some(clause.into());
        self
    }
}
