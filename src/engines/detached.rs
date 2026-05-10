use std::path::Path;

use super::EngineDriver;
use crate::migrate::manager::MigrationRow;
use crate::models::table_id::TableId;
use crate::schema::SqlDialect;
use crate::{Result, ShkiError};

pub struct Detached {
    dialect: SqlDialect,
    table: TableId,
}

impl Detached {
    pub fn new(dialect: SqlDialect, table: TableId) -> Self {
        Self { dialect, table }
    }

    pub fn with_table(mut self, table: TableId) -> Self {
        self.table = table;
        self
    }

    pub fn table(&self) -> &TableId {
        &self.table
    }

    fn unavailable(&self) -> ShkiError {
        ShkiError::config(format!(
            "Database URL is required for {} operations",
            self.dialect
        ))
    }
}

impl EngineDriver for Detached {
    async fn ensure_migrations(&self) -> Result<()> {
        Err(self.unavailable())
    }

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        Err(self.unavailable())
    }

    async fn apply_migration(&self, _path: &Path) -> Result<MigrationRow> {
        Err(self.unavailable())
    }

    async fn rollback_migration(&self, _path: &Path) -> Result<()> {
        Err(self.unavailable())
    }

    async fn mark_applied(&self, _path: &Path) -> Result<MigrationRow> {
        Err(self.unavailable())
    }

    async fn delete_table(&self) -> Result<()> {
        Err(self.unavailable())
    }
    //     Err(self.unavailable())
    // }
}
