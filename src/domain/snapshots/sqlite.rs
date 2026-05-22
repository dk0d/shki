use crate::Result;
use crate::engines::sqlite::Sqlite;
use crate::models::iden::Iden;
use crate::schema::{Column, Constraint, DbEnum, Sequence, Table, View};
use crate::snapshots::SnapshotProvider;
use indexmap::IndexMap;

#[async_trait::async_trait]
impl SnapshotProvider for Sqlite {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>> {
        todo!();
    }

    async fn get_extensions(&self, schema: &Option<String>) -> Result<Vec<String>> {
        todo!();
    }

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>> {
        todo!();
    }

    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>> {
        todo!();
    }

    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>> {
        todo!();
    }

    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<Iden, View>> {
        todo!();
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Column>>> {
        todo!();
    }
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<Constraint>>> {
        todo!();
    }
    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Index>>> {
        todo!();
    }
}
