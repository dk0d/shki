pub mod detached;
pub mod mysql;
pub mod pg;
pub mod sqlite;
mod utils;

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::ShkiError;
use crate::{Result, config::Config};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::migrate::manager::MigrationInfo;
use super::models::iden::Iden;
use super::schema::{
    Aggregate, Catalog, Column, ColumnPrivilege, CompositeType, Constraint, DbEnum,
    DefaultPrivilege, Domain, Extension, Function, Index, ObjectPrivilege, PartitionAttachment,
    Procedure, RevokedDefaultPrivilege, RowLevelSecurity, RowLevelSecurityPolicy, Sequence,
    SqlDialect, Table, Trigger, View,
};

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

    pub fn composite_types(&self) -> IndexMap<Iden, CompositeType> {
        self.catalog.flat_composite_types()
    }

    pub fn domains(&self) -> IndexMap<Iden, Domain> {
        self.catalog.flat_domains()
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

    pub fn functions(&self) -> IndexMap<Iden, Function> {
        self.catalog.flat_functions()
    }

    pub fn procedures(&self) -> IndexMap<Iden, Procedure> {
        self.catalog.flat_procedures()
    }

    pub fn aggregates(&self) -> IndexMap<Iden, Aggregate> {
        self.catalog.flat_aggregates()
    }

    pub fn triggers(&self) -> IndexMap<Iden, Trigger> {
        self.catalog.flat_triggers()
    }

    pub fn row_level_security(&self) -> IndexMap<Iden, RowLevelSecurity> {
        self.catalog.flat_row_level_security()
    }

    pub fn row_level_security_policies(&self) -> IndexMap<Iden, RowLevelSecurityPolicy> {
        self.catalog.flat_row_level_security_policies()
    }

    pub fn partition_attachments(&self) -> IndexMap<Iden, PartitionAttachment> {
        self.catalog.flat_partition_attachments()
    }

    pub fn default_privileges(&self) -> Vec<DefaultPrivilege> {
        self.catalog.default_privileges()
    }

    pub fn object_privileges(&self) -> Vec<ObjectPrivilege> {
        self.catalog.object_privileges()
    }

    pub fn column_privileges(&self) -> Vec<ColumnPrivilege> {
        self.catalog.column_privileges()
    }

    pub fn revoked_default_privileges(&self) -> Vec<RevokedDefaultPrivilege> {
        self.catalog.revoked_default_privileges()
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

    pub fn set_composite_types(&mut self, composite_types: IndexMap<Iden, CompositeType>) {
        for (id, mut composite_type) in composite_types {
            let schema_name = catalog_schema(&id.schema, &composite_type.schema);
            if id.schema.is_some() || composite_type.schema.is_some() {
                composite_type.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .composite_types
                .insert(id.name, composite_type);
        }
    }

    pub fn set_domains(&mut self, domains: IndexMap<Iden, Domain>) {
        for (id, mut domain) in domains {
            let schema_name = catalog_schema(&id.schema, &domain.schema);
            if id.schema.is_some() || domain.schema.is_some() {
                domain.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .domains
                .insert(id.name, domain);
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

    pub fn set_functions(&mut self, functions: IndexMap<Iden, Function>) {
        for (id, mut function) in functions {
            let schema_name = catalog_schema(&id.schema, &function.schema);
            if id.schema.is_some() || function.schema.is_some() {
                function.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .functions
                .insert(id.name, function);
        }
    }

    pub fn set_procedures(&mut self, procedures: IndexMap<Iden, Procedure>) {
        for (id, mut procedure) in procedures {
            let schema_name = catalog_schema(&id.schema, &procedure.schema);
            if id.schema.is_some() || procedure.schema.is_some() {
                procedure.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .procedures
                .insert(id.name, procedure);
        }
    }

    pub fn set_aggregates(&mut self, aggregates: IndexMap<Iden, Aggregate>) {
        for (id, mut aggregate) in aggregates {
            let schema_name = catalog_schema(&id.schema, &aggregate.schema);
            if id.schema.is_some() || aggregate.schema.is_some() {
                aggregate.schema = Some(schema_name.clone());
            }
            self.catalog
                .ensure_schema(schema_name)
                .aggregates
                .insert(id.name, aggregate);
        }
    }

    pub fn set_triggers(&mut self, triggers: IndexMap<Iden, Trigger>) {
        for (id, trigger) in triggers {
            let schema_name = id
                .schema
                .clone()
                .or_else(|| trigger.table.schema.clone())
                .unwrap_or_else(|| "public".to_string());
            self.catalog
                .ensure_schema(schema_name)
                .triggers
                .insert(id.name, trigger);
        }
    }

    pub fn set_row_level_security(&mut self, entries: IndexMap<Iden, RowLevelSecurity>) {
        for (id, entry) in entries {
            let schema_name = id.schema.clone().unwrap_or_else(|| "public".to_string());
            self.catalog
                .ensure_schema(schema_name)
                .row_level_security
                .insert(id.name, entry);
        }
    }

    pub fn set_row_level_security_policies(
        &mut self,
        policies: IndexMap<Iden, RowLevelSecurityPolicy>,
    ) {
        for (id, policy) in policies {
            let schema_name = id
                .schema
                .clone()
                .or_else(|| policy.table.schema.clone())
                .unwrap_or_else(|| "public".to_string());
            self.catalog
                .ensure_schema(schema_name)
                .row_level_security_policies
                .insert(id.name, policy);
        }
    }

    pub fn set_partition_attachments(&mut self, attachments: IndexMap<Iden, PartitionAttachment>) {
        for (id, attachment) in attachments {
            let schema_name = id.schema.clone().unwrap_or_else(|| "public".to_string());
            self.catalog
                .ensure_schema(schema_name)
                .partition_attachments
                .insert(id.name, attachment);
        }
    }

    pub fn set_default_privileges(&mut self, privileges: IndexMap<String, Vec<DefaultPrivilege>>) {
        for (schema_name, privileges) in privileges {
            self.catalog.ensure_schema(schema_name).default_privileges = privileges;
        }
    }

    pub fn set_object_privileges(&mut self, privileges: IndexMap<String, Vec<ObjectPrivilege>>) {
        for (schema_name, privileges) in privileges {
            self.catalog.ensure_schema(schema_name).object_privileges = privileges;
        }
    }

    pub fn set_column_privileges(&mut self, privileges: IndexMap<String, Vec<ColumnPrivilege>>) {
        for (schema_name, privileges) in privileges {
            self.catalog.ensure_schema(schema_name).column_privileges = privileges;
        }
    }

    pub fn set_revoked_default_privileges(
        &mut self,
        privileges: IndexMap<String, Vec<RevokedDefaultPrivilege>>,
    ) {
        for (schema_name, privileges) in privileges {
            self.catalog
                .ensure_schema(schema_name)
                .revoked_default_privileges = privileges;
        }
    }

    pub fn insert_table(&mut self, id: Iden, table: Table) {
        let mut tables = IndexMap::new();
        tables.insert(id, table);
        self.set_tables(tables);
    }

    pub fn parse(content: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(content)?)
    }

    pub fn from_json(snapshot_path: &PathBuf) -> crate::Result<Self> {
        let content = std::fs::read_to_string(snapshot_path).map_err(|err| {
            ShkiError::schema(format!(
                "Failed to read Snapshot {}: {}",
                snapshot_path.display(),
                err
            ))
        })?;
        Self::parse(&content)
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
    async fn get_composite_types(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, CompositeType>> {
        Ok(IndexMap::new())
    }
    async fn get_domains(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Domain>> {
        Ok(IndexMap::new())
    }
    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>>;
    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>>;
    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<Iden, View>>;
    async fn get_functions(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Function>> {
        Ok(IndexMap::new())
    }
    async fn get_procedures(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Procedure>> {
        Ok(IndexMap::new())
    }
    async fn get_aggregates(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Aggregate>> {
        Ok(IndexMap::new())
    }
    async fn get_triggers(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Trigger>> {
        Ok(IndexMap::new())
    }
    async fn get_row_level_security(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, RowLevelSecurity>> {
        Ok(IndexMap::new())
    }
    async fn get_row_level_security_policies(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, RowLevelSecurityPolicy>> {
        Ok(IndexMap::new())
    }
    async fn get_partition_attachments(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, PartitionAttachment>> {
        Ok(IndexMap::new())
    }
    async fn get_default_privileges(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<DefaultPrivilege>>> {
        Ok(IndexMap::new())
    }
    async fn get_object_privileges(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<ObjectPrivilege>>> {
        Ok(IndexMap::new())
    }
    async fn get_column_privileges(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<ColumnPrivilege>>> {
        Ok(IndexMap::new())
    }
    async fn get_revoked_default_privileges(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<String, Vec<RevokedDefaultPrivilege>>> {
        Ok(IndexMap::new())
    }
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

#[async_trait::async_trait]
impl<E> Introspectable for E
where
    E: SnapshotProvider + Send + Sync,
{
    async fn introspect(&self, config: &Config, schema: &Option<String>) -> Result<Snapshot> {
        let mut snapshot = Snapshot::new(config.dialect());

        let schema = match config.dialect() {
            SqlDialect::Postgres => Some(schema.clone().unwrap_or("public".to_string())),
            SqlDialect::Mysql | SqlDialect::Sqlite => schema.clone(),
        };

        snapshot.set_schemas(self.get_schemas(&schema).await?);
        snapshot.set_enums(self.get_enums(&schema).await?);
        snapshot.set_composite_types(self.get_composite_types(&schema).await?);
        snapshot.set_domains(self.get_domains(&schema).await?);
        snapshot.set_views(self.get_views(&schema).await?);
        snapshot.set_functions(self.get_functions(&schema).await?);
        snapshot.set_procedures(self.get_procedures(&schema).await?);
        snapshot.set_aggregates(self.get_aggregates(&schema).await?);
        snapshot.set_triggers(self.get_triggers(&schema).await?);
        snapshot.set_row_level_security(self.get_row_level_security(&schema).await?);
        snapshot
            .set_row_level_security_policies(self.get_row_level_security_policies(&schema).await?);
        snapshot.set_partition_attachments(self.get_partition_attachments(&schema).await?);
        snapshot.set_default_privileges(self.get_default_privileges(&schema).await?);
        snapshot.set_object_privileges(self.get_object_privileges(&schema).await?);
        snapshot.set_column_privileges(self.get_column_privileges(&schema).await?);
        snapshot
            .set_revoked_default_privileges(self.get_revoked_default_privileges(&schema).await?);
        snapshot.set_sequences(self.get_sequences(&schema).await?);
        snapshot.set_extensions(self.get_extensions(&schema).await?);

        let mut tables = self.get_tables(&schema).await?;
        let constraints = self.get_constraints(&schema).await?;
        let columns = self.get_columns(&schema).await?;
        let indexes = self.get_indexes(&schema).await?;

        attach_columns(&mut tables, columns);
        attach_constraints(&mut tables, constraints);
        attach_indexes(&mut tables, indexes);
        snapshot.set_tables(tables);

        Ok(snapshot)
    }
}

fn attach_columns(
    tables: &mut IndexMap<Iden, Table>,
    columns: IndexMap<Iden, IndexMap<String, Column>>,
) {
    for (table_id, columns) in columns {
        if let Some(table) = tables.get_mut(&table_id) {
            table.columns = columns;
        }
    }
}

fn attach_constraints(
    tables: &mut IndexMap<Iden, Table>,
    constraints: IndexMap<Iden, Vec<Constraint>>,
) {
    for (table_id, constraints) in constraints {
        if let Some(table) = tables.get_mut(&table_id) {
            table.constraints = constraints;
        }
    }
}

fn attach_indexes(
    tables: &mut IndexMap<Iden, Table>,
    indexes: IndexMap<Iden, IndexMap<String, Index>>,
) {
    for (table_id, indexes) in indexes {
        if let Some(table) = tables.get_mut(&table_id) {
            table.indexes = indexes;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::schema::{Column, DataType, SqlDialect, Table};

    #[test]
    fn snapshot_json_uses_catalog_shaped_contract() {
        let mut snapshot = Snapshot::new(SqlDialect::Postgres);
        snapshot.prev_id = Some("previous-snapshot".to_string());
        let mut table = Table::in_schema("users", "app");
        table.column(Column::new("id", DataType::Integer).not_null());
        snapshot.insert_table(Iden::new("users", Some("app".to_string())), table);

        let json = snapshot.to_json().expect("snapshot should serialize");
        let value: Value = serde_json::from_str(&json).expect("snapshot json should parse");

        assert!(value.get("catalog").is_some());
        assert_eq!(value["prevId"], "previous-snapshot");
        assert!(value.get("createdAt").is_some());
        assert!(value["catalog"]["schemas"]["app"]["tables"]["users"].is_object());
        assert!(value.get("tables").is_none());
        assert!(value.get("enums").is_none());
        assert!(value.get("sequences").is_none());
        assert!(value.get("views").is_none());
    }
}
