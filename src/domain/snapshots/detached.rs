#![allow(unused_variables)]
use indexmap::IndexMap;

use crate::Result;
use crate::engines::detached::Detached;
use crate::models::iden::Iden;
use crate::schema::{Column, Constraint, DbEnum, Sequence, Table, View};
use crate::snapshots::SnapshotProvider;

#[async_trait::async_trait]
impl SnapshotProvider for Detached {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>> {
        Err(self.unavailable())
    }

    async fn get_extensions(&self, schema: &Option<String>) -> Result<Vec<String>> {
        Err(self.unavailable())
    }

    async fn get_enums(&self, schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>> {
        Err(self.unavailable())
    }

    async fn get_sequences(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>> {
        Err(self.unavailable())
    }

    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>> {
        Err(self.unavailable())
    }

    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<Iden, View>> {
        Err(self.unavailable())
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Column>>> {
        Err(self.unavailable())
    }
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<Constraint>>> {
        Err(self.unavailable())
    }
    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Index>>> {
        Err(self.unavailable())
    }
}
