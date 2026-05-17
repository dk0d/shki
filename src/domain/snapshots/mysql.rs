use crate::Result;
use crate::engines::mysql::Mysql;
use crate::models::table_id::TableId;
use crate::schema::{Column, Constraint, DbEnum, Sequence, Table, View};
use crate::snapshots::SnapshotProvider;
use indexmap::IndexMap;

#[async_trait::async_trait]
impl SnapshotProvider for Mysql {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>> {
        todo!();
    }

    async fn get_extensions(&self, schema: &Option<String>) -> Result<Vec<String>> {
        todo!();
    }

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<String, DbEnum>> {
        todo!();
    }

    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<String, Sequence>> {
        todo!();
    }

    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<TableId, Table>> {
        todo!();
    }

    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<TableId, View>> {
        todo!();
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, IndexMap<String, Column>>> {
        todo!();
    }
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, Vec<Constraint>>> {
        todo!();
    }
    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<TableId, Vec<crate::schema::Index>>> {
        todo!();
    }
}
