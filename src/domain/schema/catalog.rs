use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::models::iden::Iden;

use super::{
    Aggregate, ColumnPrivilege, CompositeType, DbEnum, DefaultPrivilege, Domain, Extension,
    Function, ObjectPrivilege, PartitionAttachment, Procedure, RevokedDefaultPrivilege,
    RowLevelSecurity, RowLevelSecurityPolicy, Sequence, Table, Trigger, View,
};

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
    pub composite_types: IndexMap<String, CompositeType>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub domains: IndexMap<String, Domain>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sequences: IndexMap<String, Sequence>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub views: IndexMap<String, View>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub functions: IndexMap<String, Function>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub procedures: IndexMap<String, Procedure>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub aggregates: IndexMap<String, Aggregate>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub triggers: IndexMap<String, Trigger>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub row_level_security: IndexMap<String, RowLevelSecurity>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub row_level_security_policies: IndexMap<String, RowLevelSecurityPolicy>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub partition_attachments: IndexMap<String, PartitionAttachment>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_privileges: Vec<DefaultPrivilege>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_privileges: Vec<ObjectPrivilege>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_privileges: Vec<ColumnPrivilege>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revoked_default_privileges: Vec<RevokedDefaultPrivilege>,
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

    pub fn flat_composite_types(&self) -> IndexMap<Iden, CompositeType> {
        let mut composite_types = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, composite_type) in &schema.composite_types {
                composite_types.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &composite_type.schema),
                    ),
                    composite_type.clone(),
                );
            }
        }
        composite_types
    }

    pub fn flat_domains(&self) -> IndexMap<Iden, Domain> {
        let mut domains = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, domain) in &schema.domains {
                domains.insert(
                    Iden::new(
                        name.clone(),
                        object_schema_for_id(schema_name, &domain.schema),
                    ),
                    domain.clone(),
                );
            }
        }
        domains
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

    pub fn flat_functions(&self) -> IndexMap<Iden, Function> {
        let mut functions = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (signature, function) in &schema.functions {
                functions.insert(
                    Iden::new(
                        signature.clone(),
                        object_schema_for_id(schema_name, &function.schema),
                    ),
                    function.clone(),
                );
            }
        }
        functions
    }

    pub fn flat_procedures(&self) -> IndexMap<Iden, Procedure> {
        let mut procedures = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (signature, procedure) in &schema.procedures {
                procedures.insert(
                    Iden::new(
                        signature.clone(),
                        object_schema_for_id(schema_name, &procedure.schema),
                    ),
                    procedure.clone(),
                );
            }
        }
        procedures
    }

    pub fn flat_aggregates(&self) -> IndexMap<Iden, Aggregate> {
        let mut aggregates = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (signature, aggregate) in &schema.aggregates {
                aggregates.insert(
                    Iden::new(
                        signature.clone(),
                        object_schema_for_id(schema_name, &aggregate.schema),
                    ),
                    aggregate.clone(),
                );
            }
        }
        aggregates
    }

    pub fn flat_triggers(&self) -> IndexMap<Iden, Trigger> {
        let mut triggers = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, trigger) in &schema.triggers {
                triggers.insert(
                    Iden::new(name.clone(), object_schema_for_id(schema_name, &None)),
                    trigger.clone(),
                );
            }
        }
        triggers
    }

    pub fn flat_row_level_security(&self) -> IndexMap<Iden, RowLevelSecurity> {
        let mut entries = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (table_name, rls) in &schema.row_level_security {
                entries.insert(
                    Iden::new(table_name.clone(), Some(schema_name.clone())),
                    rls.clone(),
                );
            }
        }
        entries
    }

    pub fn flat_row_level_security_policies(&self) -> IndexMap<Iden, RowLevelSecurityPolicy> {
        let mut policies = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, policy) in &schema.row_level_security_policies {
                policies.insert(
                    Iden::new(name.clone(), Some(schema_name.clone())),
                    policy.clone(),
                );
            }
        }
        policies
    }

    pub fn flat_partition_attachments(&self) -> IndexMap<Iden, PartitionAttachment> {
        let mut attachments = IndexMap::new();
        for (schema_name, schema) in &self.schemas {
            for (name, attachment) in &schema.partition_attachments {
                attachments.insert(
                    Iden::new(name.clone(), Some(schema_name.clone())),
                    attachment.clone(),
                );
            }
        }
        attachments
    }

    pub fn default_privileges(&self) -> Vec<DefaultPrivilege> {
        self.schemas
            .values()
            .flat_map(|schema| schema.default_privileges.iter().cloned())
            .collect()
    }

    pub fn object_privileges(&self) -> Vec<ObjectPrivilege> {
        self.schemas
            .values()
            .flat_map(|schema| schema.object_privileges.iter().cloned())
            .collect()
    }

    pub fn column_privileges(&self) -> Vec<ColumnPrivilege> {
        self.schemas
            .values()
            .flat_map(|schema| schema.column_privileges.iter().cloned())
            .collect()
    }

    pub fn revoked_default_privileges(&self) -> Vec<RevokedDefaultPrivilege> {
        self.schemas
            .values()
            .flat_map(|schema| schema.revoked_default_privileges.iter().cloned())
            .collect()
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
            composite_types: IndexMap::new(),
            domains: IndexMap::new(),
            sequences: IndexMap::new(),
            views: IndexMap::new(),
            functions: IndexMap::new(),
            procedures: IndexMap::new(),
            aggregates: IndexMap::new(),
            triggers: IndexMap::new(),
            row_level_security: IndexMap::new(),
            row_level_security_policies: IndexMap::new(),
            partition_attachments: IndexMap::new(),
            default_privileges: Vec::new(),
            object_privileges: Vec::new(),
            column_privileges: Vec::new(),
            revoked_default_privileges: Vec::new(),
        }
    }
}
