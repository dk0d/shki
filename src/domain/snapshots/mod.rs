pub mod detached;
pub mod mysql;
pub mod pg;
pub mod sqlite;
mod utils;

use chrono::{DateTime, Utc};

use crate::{Result, config::Config};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::migrate::manager::MigrationInfo;
use super::models::iden::Iden;
use super::schema::{Catalog, Column, Constraint, DbEnum, Extension, Index, Sequence, SqlDialect, Table, View};

/// A snapshot of a database schema
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

    /// Normalized database shape for this Snapshot.
    #[serde(default)]
    pub catalog: Catalog,
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
            catalog: Catalog::default(),
        }
    }

    pub fn schemas(&self) -> Vec<String> {
        self.catalog.schema_names()
    }

    pub fn extensions(&self) -> Vec<String> {
        self.catalog.extension_names()
    }

    pub fn enums(&self) -> IndexMap<Iden, DbEnum> {
        self.catalog.flat_enums()
    }

    pub fn sequences(&self) -> IndexMap<Iden, Sequence> {
        self.catalog.flat_sequences()
    }

    pub fn tables(&self) -> IndexMap<Iden, Table> {
        self.catalog.flat_tables()
    }

    pub fn views(&self) -> IndexMap<Iden, View> {
        self.catalog.flat_views()
    }

    pub fn set_schemas(&mut self, schemas: Vec<String>) {
        for schema in schemas {
            self.catalog.ensure_schema(schema);
        }
    }

    pub fn set_extensions(&mut self, extensions: Vec<String>) {
        self.catalog.extensions = extensions
            .into_iter()
            .map(|name| (name.clone(), Extension::new(name)))
            .collect();
    }

    pub fn set_enums(&mut self, enums: IndexMap<Iden, DbEnum>) {
        for (id, mut db_enum) in enums {
            let schema_name = catalog_schema(&id.schema, &db_enum.schema);
            if id.schema.is_some() || db_enum.schema.is_some() {
                db_enum.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .enums
                .insert(id.name, db_enum);
        }
    }

    pub fn set_sequences(&mut self, sequences: IndexMap<Iden, Sequence>) {
        for (id, mut sequence) in sequences {
            let schema_name = catalog_schema(&id.schema, &sequence.schema);
            if id.schema.is_some() || sequence.schema.is_some() {
                sequence.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .sequences
                .insert(id.name, sequence);
        }
    }

    pub fn set_tables(&mut self, tables: IndexMap<Iden, Table>) {
        for (id, mut table) in tables {
            let schema_name = catalog_schema(&id.schema, &table.schema);
            if id.schema.is_some() || table.schema.is_some() {
                table.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .tables
                .insert(id.name, table);
        }
    }

    pub fn set_views(&mut self, views: IndexMap<Iden, View>) {
        for (id, mut view) in views {
            let schema_name = catalog_schema(&id.schema, &view.schema);
            if id.schema.is_some() || view.schema.is_some() {
                view.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .views
                .insert(id.name, view);
        }
    }

    pub fn insert_table(&mut self, id: Iden, table: Table) {
        let mut tables = IndexMap::new();
        tables.insert(id, table);
        self.set_tables(tables);
    }

    /// Save snapshot to JSON
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

fn catalog_schema(id_schema: &Option<String>, object_schema: &Option<String>) -> String {
    id_schema
        .clone()
        .or_else(|| object_schema.clone())
        .unwrap_or_else(|| "public".to_string())
}

#[async_trait::async_trait]

pub trait Introspectable {
    async fn introspect(&self, config: &Config, schema: &Option<String>) -> Result<Snapshot>;
}

#[async_trait::async_trait]
#[enum_dispatch::enum_dispatch(Engine)]
pub trait SnapshotProvider {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>>;
    async fn get_extensions(&self, schema: &Option<String>) -> Result<Vec<String>>;
    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>>;
    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>>;
    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>>;
    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<Iden, View>>;
    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Column>>>;
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<Constraint>>>;
    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Index>>>;
}
