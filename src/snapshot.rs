//! Schema snapshots for tracking schema state
//!
//! Snapshots are JSON representations of the database schema at a point in time.
//! They are used to compute diffs between schema versions.

use chrono::{DateTime, Utc};
use colored::Colorize;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::ShkiError;
use crate::schema::{Column, Constraint, Index, Schema, SchemaDialect, Table};

/// A snapshot of a database schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version
    pub version: String,

    /// Unique identifier for this snapshot
    pub id: String,

    /// Previous snapshot ID (for migration chain)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_id: Option<String>,

    /// Database dialect
    pub dialect: SchemaDialect,

    /// Timestamp when the snapshot was created
    pub created_at: DateTime<Utc>,

    /// Migration that produced this snapshot (name and checksum)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationInfo>,

    /// Tables in the schema
    #[serde(default)]
    pub tables: IndexMap<String, TableSnapshot>,

    /// Enums in the schema
    #[serde(default)]
    pub enums: IndexMap<String, EnumSnapshot>,

    /// Sequences in the schema
    #[serde(default)]
    pub sequences: IndexMap<String, SequenceSnapshot>,

    /// Views in the schema
    #[serde(default)]
    pub views: IndexMap<String, ViewSnapshot>,

    /// Schemas (PostgreSQL)
    #[serde(default)]
    pub schemas: Vec<String>,

    /// Extensions (PostgreSQL)
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl From<Schema> for Snapshot {
    fn from(schema: Schema) -> Self {
        Snapshot::from_schema(&schema)
    }
}

/// Snapshot of a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSnapshot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub columns: IndexMap<String, ColumnSnapshot>,
    #[serde(default)]
    pub constraints: Vec<ConstraintSnapshot>,
    #[serde(default)]
    pub indexes: IndexMap<String, IndexSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Snapshot of a column
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnSnapshot {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
}

/// Snapshot of a constraint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConstraintSnapshot {
    pub name: Option<String>,
    pub constraint_type: ConstraintType,
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<ForeignKeyReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
}

/// Constraint type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    PrimaryKey,
    Unique,
    ForeignKey,
    Check,
    Exclusion,
}

/// Foreign key reference info
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForeignKeyReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
}

/// Snapshot of an index
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexSnapshot {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default = "default_btree")]
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
}

fn default_btree() -> String {
    "btree".to_string()
}

/// Snapshot of an enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumSnapshot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Snapshot of a sequence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SequenceSnapshot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub increment: i64,
    pub min_value: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
    pub start: i64,
    pub cache: i64,
    pub cycle: bool,
}

/// Snapshot of a view
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewSnapshot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub definition: String,
    #[serde(default)]
    pub materialized: bool,
}

/// Information about a migration that produced this snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationInfo {
    /// Name of the migration file (without .sql extension)
    pub name: String,
    /// SHA-256 checksum of the migration SQL content
    pub checksum: String,
}

impl Snapshot {
    /// Create a new empty snapshot
    pub fn new(dialect: SchemaDialect) -> Self {
        Self {
            version: "1".to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            prev_id: None,
            dialect,
            created_at: Utc::now(),
            migration: None,
            tables: IndexMap::new(),
            enums: IndexMap::new(),
            sequences: IndexMap::new(),
            views: IndexMap::new(),
            schemas: Vec::new(),
            extensions: Vec::new(),
        }
    }

    /// Create a snapshot from a schema
    pub fn from_schema(schema: &Schema) -> Self {
        let mut snapshot = Self::new(schema.dialect);
        snapshot.schemas.push(schema.name.clone());
        snapshot.extensions = schema.extensions.clone();

        for (name, table) in &schema.tables {
            snapshot.tables.insert(
                name.clone(),
                TableSnapshot::from_table(table, schema.dialect),
            );
        }

        for (name, enum_type) in &schema.enums {
            snapshot.enums.insert(
                name.clone(),
                EnumSnapshot {
                    name: enum_type.name.clone(),
                    schema: enum_type.schema.clone(),
                    values: enum_type.values.clone(),
                    description: enum_type.description.clone(),
                },
            );
        }

        for (name, sequence) in &schema.sequences {
            snapshot.sequences.insert(
                name.clone(),
                SequenceSnapshot {
                    name: sequence.name.clone(),
                    schema: sequence.schema.clone(),
                    increment: sequence.increment,
                    min_value: sequence.min_value,
                    max_value: sequence.max_value,
                    start: sequence.start,
                    cache: sequence.cache,
                    cycle: sequence.cycle,
                },
            );
        }

        for (name, view) in &schema.views {
            snapshot.views.insert(
                name.clone(),
                ViewSnapshot {
                    name: view.name.clone(),
                    schema: view.schema.clone(),
                    definition: view.definition.clone(),
                    materialized: view.materialized,
                },
            );
        }

        snapshot
    }

    /// Set migration info for this snapshot
    pub fn with_migration(mut self, name: impl Into<String>, checksum: impl Into<String>) -> Self {
        self.migration = Some(MigrationInfo {
            name: name.into(),
            checksum: checksum.into(),
        });
        self
    }

    /// Load a snapshot from JSON
    pub fn from_json(json: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    /// Save snapshot to JSON
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Load schema from config.schema path(s)
    ///
    /// This function resolves the glob patterns in config.schema and loads/merges
    /// all matching schema files into a single Snapshot.
    pub fn from_config(config: &crate::config::Config) -> crate::Result<Self> {
        if config.schema.is_empty() {
            return Err(ShkiError::config(
                "No schema files found. Either:\n  \
                     - Provide a schema path with --schema <path>\n  \
                     - Configure schema patterns in shki.toml under 'schema'",
            ));
        }
        let path = config.schema_path();
        Self::from_path(&path)
    }

    /// Load a schema snapshot from a file path
    ///
    /// Supports:
    /// - `.lua` files (requires `lua` feature)
    /// - `.json` files (snapshot format)
    pub fn from_path(path: &std::path::Path) -> crate::Result<Self> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match extension {
            "lua" => {
                println!("{} {}", "Loading Lua schema:".cyan(), path.display());
                let schema = crate::lua::load_schema_from_file(path)?;
                Ok(Snapshot::from_schema(&schema))
            }
            "json" => {
                let content = std::fs::read_to_string(path)?;
                Snapshot::from_json(&content)
            }
            _ => Err(ShkiError::config(format!(
                "Unsupported schema file extension: '{}'. Supported: .lua, .json",
                extension
            ))),
        }
    }

    /// Load the latest snapshot from a directory
    pub fn load_latest(dir: &std::path::Path) -> crate::Result<Option<Self>> {
        let meta_dir = dir.join("_meta");
        if !meta_dir.exists() {
            return Ok(None);
        }

        let mut latest: Option<(Self, std::path::PathBuf)> = None;

        for entry in std::fs::read_dir(&meta_dir)? {
            let entry = entry?;
            let path = entry.path();

            let is_json = path.extension().map(|ext| ext == "json").unwrap_or(false);
            if !is_json {
                continue;
            }

            let content = std::fs::read_to_string(&path)?;
            let snapshot = Self::from_json(&content)?;

            let should_replace = match &latest {
                None => true,
                Some((current, current_path)) => {
                    snapshot.created_at > current.created_at
                        || (snapshot.created_at == current.created_at && path > *current_path)
                }
            };

            if should_replace {
                latest = Some((snapshot, path));
            }
        }

        Ok(latest.map(|(snapshot, _)| snapshot))
    }

    /// Load all snapshots from a directory
    ///
    /// Returns snapshots sorted by creation time (oldest first).
    pub fn load_all(dir: &std::path::Path) -> crate::Result<Vec<Self>> {
        let meta_dir = dir.join("_meta");
        if !meta_dir.exists() {
            return Ok(Vec::new());
        }

        let entries: Vec<_> = std::fs::read_dir(&meta_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();

        let mut snapshots_with_path = Vec::new();
        for entry in entries {
            let path = entry.path();
            let content = std::fs::read_to_string(&path)?;
            let snapshot = Self::from_json(&content)?;
            snapshots_with_path.push((snapshot, path));
        }

        // Sort by snapshot creation time (oldest first).
        // Fall back to file path for deterministic ordering when timestamps are equal.
        snapshots_with_path.sort_by(|(a, a_path), (b, b_path)| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a_path.cmp(b_path))
        });

        Ok(snapshots_with_path
            .into_iter()
            .map(|(snapshot, _)| snapshot)
            .collect())
    }

    /// Save snapshot to a directory
    pub fn save(&self, dir: &std::path::Path) -> crate::Result<std::path::PathBuf> {
        let meta_dir = dir.join("_meta");
        std::fs::create_dir_all(&meta_dir)?;

        let filename = format!(
            "{}_{}.json",
            self.created_at.format("%Y%m%d%H%M%S"),
            &self.id[..8]
        );
        let path = meta_dir.join(filename);
        let json = self.to_json()?;
        std::fs::write(&path, json)?;
        Ok(path)
    }
}

impl TableSnapshot {
    /// Create a table snapshot from a table definition
    pub fn from_table(table: &Table, dialect: SchemaDialect) -> Self {
        let mut columns = IndexMap::new();
        for (name, col) in &table.columns {
            columns.insert(name.clone(), ColumnSnapshot::from_column(col, dialect));
        }

        let mut constraints = Vec::new();
        for constraint in &table.constraints {
            constraints.push(ConstraintSnapshot::from_constraint(constraint));
        }

        // Convert column-level references to FK constraints
        for col in table.columns.values() {
            if let Some(ref col_ref) = col.references {
                constraints.push(ConstraintSnapshot {
                    name: Some(format!("{}_{}_fkey", table.name, col.name)),
                    constraint_type: ConstraintType::ForeignKey,
                    columns: vec![col.name.clone()],
                    references: Some(ForeignKeyReference {
                        schema: None,
                        table: col_ref.table.clone(),
                        columns: vec![col_ref.column.clone()],
                        on_delete: col_ref.on_delete.to_sql().to_string(),
                        on_update: col_ref.on_update.to_sql().to_string(),
                    }),
                    expression: None,
                });
            }
        }

        let mut indexes = IndexMap::new();
        for (name, idx) in &table.indexes {
            indexes.insert(name.clone(), IndexSnapshot::from_index(idx));
        }

        Self {
            name: table.name.clone(),
            schema: table.schema.clone(),
            columns,
            constraints,
            indexes,
            comment: table.comment.clone(),
        }
    }
}

impl ColumnSnapshot {
    /// Create a column snapshot from a column definition
    pub fn from_column(column: &Column, dialect: SchemaDialect) -> Self {
        let data_type = match dialect {
            SchemaDialect::Postgres => column.data_type.to_postgres_sql(),
            SchemaDialect::Mysql => column.data_type.to_mysql_sql(),
            SchemaDialect::Sqlite => column.data_type.to_sqlite_sql(),
        };

        let default = column.default.as_ref().map(|d| match d {
            crate::schema::DefaultValue::Literal(v) => format!("'{}'", v),
            crate::schema::DefaultValue::Sql(e) => e.clone(),
            crate::schema::DefaultValue::Null => "NULL".to_string(),
            crate::schema::DefaultValue::Sequence(s) => format!("nextval('{}')", s),
            crate::schema::DefaultValue::Identity { always } => {
                if *always {
                    "GENERATED ALWAYS AS IDENTITY".to_string()
                } else {
                    "GENERATED BY DEFAULT AS IDENTITY".to_string()
                }
            }
        });

        let generated = column.generated.as_ref().map(|g| {
            if g.stored {
                format!("GENERATED ALWAYS AS ({}) STORED", g.expression)
            } else {
                format!("GENERATED ALWAYS AS ({})", g.expression)
            }
        });

        let identity = column.identity.as_ref().map(|i| {
            if i.always {
                "ALWAYS".to_string()
            } else {
                "BY DEFAULT".to_string()
            }
        });

        Self {
            name: column.name.clone(),
            data_type,
            nullable: column.nullable,
            default,
            primary_key: column.primary_key,
            unique: column.unique,
            generated,
            identity,
            comment: column.comment.clone(),
            collation: column.collation.clone(),
        }
    }
}

impl ConstraintSnapshot {
    /// Create a constraint snapshot from a constraint definition
    pub fn from_constraint(constraint: &Constraint) -> Self {
        match constraint {
            Constraint::PrimaryKey(pk) => Self {
                name: pk.name.clone(),
                constraint_type: ConstraintType::PrimaryKey,
                columns: pk.columns.clone(),
                references: None,
                expression: None,
            },
            Constraint::Unique(u) => Self {
                name: u.name.clone(),
                constraint_type: ConstraintType::Unique,
                columns: u.columns.clone(),
                references: None,
                expression: None,
            },
            Constraint::ForeignKey(fk) => Self {
                name: fk.name.clone(),
                constraint_type: ConstraintType::ForeignKey,
                columns: fk.columns.clone(),
                references: Some(ForeignKeyReference {
                    schema: fk.references_schema.clone(),
                    table: fk.references_table.clone(),
                    columns: fk.references_columns.clone(),
                    on_delete: fk.on_delete.to_sql().to_string(),
                    on_update: fk.on_update.to_sql().to_string(),
                }),
                expression: None,
            },
            Constraint::Check(c) => Self {
                name: c.name.clone(),
                constraint_type: ConstraintType::Check,
                columns: Vec::new(),
                references: None,
                expression: Some(c.expression.clone()),
            },
            Constraint::Exclusion(e) => Self {
                name: e.name.clone(),
                constraint_type: ConstraintType::Exclusion,
                columns: e.elements.iter().map(|(c, _)| c.clone()).collect(),
                references: None,
                expression: e.where_clause.clone(),
            },
        }
    }
}

impl IndexSnapshot {
    /// Create an index snapshot from an index definition
    pub fn from_index(index: &Index) -> Self {
        let columns: Vec<String> = index
            .columns
            .iter()
            .map(|c| match c {
                crate::schema::IndexColumn::Column { name, .. } => name.clone(),
                crate::schema::IndexColumn::Expression { expression, .. } => {
                    format!("({})", expression)
                }
            })
            .collect();

        Self {
            name: index.name.clone(),
            columns,
            unique: index.unique,
            method: index.method.to_sql().to_string(),
            where_clause: index.where_clause.clone(),
            include: index.include.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    #[test]
    fn load_latest_prefers_snapshot_created_at_over_filename_order() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let meta_dir = dir.path().join("_meta");
        std::fs::create_dir_all(&meta_dir).expect("failed to create _meta");

        let older = Snapshot::new(SchemaDialect::Postgres);
        let mut newer = Snapshot::new(SchemaDialect::Postgres);
        newer.created_at = older.created_at + Duration::seconds(1);

        // Intentionally write filenames in reverse lexical order relative to created_at.
        std::fs::write(
            meta_dir.join("99999999999999_old.json"),
            older.to_json().expect("failed to serialize older snapshot"),
        )
        .expect("failed to write older snapshot");
        std::fs::write(
            meta_dir.join("00000000000000_new.json"),
            newer.to_json().expect("failed to serialize newer snapshot"),
        )
        .expect("failed to write newer snapshot");

        let latest = Snapshot::load_latest(dir.path())
            .expect("failed to load latest snapshot")
            .expect("latest snapshot should exist");

        assert_eq!(latest.id, newer.id);
    }
}
