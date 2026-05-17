pub mod detached;
pub mod mysql;
pub mod pg;
pub mod sqlite;

use chrono::{DateTime, Utc};

use crate::{Result, config::Config};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::migrate::manager::MigrationInfo;
use super::models::table_id::TableId;
use super::schema::{Column, Constraint, DbEnum, Index, Sequence, SqlDialect, Table, View};

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
    pub dialect: SqlDialect,

    /// Timestamp when the snapshot was created
    pub created_at: DateTime<Utc>,

    /// Migration that produced this snapshot (name and checksum)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationInfo>,

    /// Tables in the schema
    #[serde(default)]
    pub tables: IndexMap<TableId, Table>,

    /// Enums in the schema
    #[serde(default)]
    pub enums: IndexMap<String, DbEnum>,

    /// Sequences in the schema
    #[serde(default)]
    pub sequences: IndexMap<String, Sequence>,

    /// Views in the schema
    #[serde(default)]
    pub views: IndexMap<String, View>,

    /// Schemas (PostgreSQL)
    #[serde(default)]
    pub schemas: Vec<String>,

    /// Extensions (PostgreSQL)
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl Snapshot {
    pub fn new(dialect: SqlDialect) -> Self {
        Snapshot {
            version: "1.0".to_string(),
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
}

#[async_trait::async_trait]

pub trait Introspectable {
    async fn introspect(&self, config: &Config) -> Result<Snapshot>;
}

#[async_trait::async_trait]
#[enum_dispatch::enum_dispatch(Engine)]
pub trait SnapshotProvider {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>>;
    async fn get_extensions(&self, schema: &Option<String>) -> Result<Vec<String>>;
    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<String, DbEnum>>;
    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<String, Sequence>>;
    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<TableId, Table>>;
    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<TableId, View>>;
    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, IndexMap<String, Column>>>;
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, Vec<Constraint>>>;
    async fn get_indexes(&self, schema: &Option<String>) -> Result<IndexMap<TableId, Vec<Index>>>;
}
