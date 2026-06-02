use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::models::iden::Iden;

use super::{DbEnum, Extension, Function, Sequence, Table, Trigger, View};

/// Normalized database shape stored inside a Snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// PostgreSQL extensions keyed by extension name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extensions: IndexMap<String, Extension>,

    /// Database schemas keyed by schema name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub schemas: IndexMap<String, CatalogSchema>,
}

/// Schema-scoped objects in a Catalog.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSchema {
    pub name: String,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub tables: IndexMap<String, Table>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub enums: IndexMap<String, DbEnum>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sequences: IndexMap<String, Sequence>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub views: IndexMap<String, View>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub functions: IndexMap<String, Function>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub triggers: IndexMap<String, Trigger>,
}

impl Catalog {
    pub fn ensure_schema(&mut self, name: impl Into<String>) -> &mut CatalogSchema {
        let name = name.into();
        self.schemas
            .entry(name.clone())
            .or_insert_with(|| CatalogSchema::new(name))
    }

    pub fn schema_names(&self) -> Vec<String> {
        self.schemas.keys().cloned().collect()
    }

    pub fn extension_names(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }

    pub fn flat_tables(&self) -> IndexMap<Iden, Table> {
        let mut tables = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, table) in &schema.tables {
                tables.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &table.schema),
                    ),
                    table.clone(),
                );
            }
        }
        tables
    }

    pub fn flat_enums(&self) -> IndexMap<Iden, DbEnum> {
        let mut enums = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, db_enum) in &schema.enums {
                enums.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &db_enum.schema),
                    ),
                    db_enum.clone(),
                );
            }
        }
        enums
    }

    pub fn flat_sequences(&self) -> IndexMap<Iden, Sequence> {
        let mut sequences = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, sequence) in &schema.sequences {
                sequences.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &sequence.schema),
                    ),
                    sequence.clone(),
                );
            }
        }
        sequences
    }

    pub fn flat_views(&self) -> IndexMap<Iden, View> {
        let mut views = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, view) in &schema.views {
                views.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &view.schema),
                    ),
                    view.clone(),
                );
            }
        }
        views
    }
}

fn object_schema_for_id(schema_name: &str, object_schema: &Option<String>) -> Option<String> {
    object_schema
        .clone()
        .or_else(|| (schema_name != "public").then(|| schema_name.to_string()))
}

impl CatalogSchema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tables: IndexMap::new(),
            enums: IndexMap::new(),
            sequences: IndexMap::new(),
            views: IndexMap::new(),
            functions: IndexMap::new(),
            triggers: IndexMap::new(),
        }
    }
}
