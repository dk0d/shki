use indexmap::IndexMap;
use std::collections::HashSet;

use crate::Result;
use crate::schema::*;

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
        prev: DbEnum,
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
    SetDefault(String),
    DropDefault,
    SetGenerated(String),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
