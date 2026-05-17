pub mod detached;
pub mod mysql;
pub mod pg;
pub mod queries;
pub mod sqlite;
pub mod utils;

use indexmap::IndexMap;

use self::detached::Detached;
use self::mysql::Mysql;
use self::pg::Postgres;
use self::sqlite::Sqlite;
use std::path::Path;
use std::pin::Pin;

use crate::Result;
use crate::config::Config;
use crate::migrate::manager::MigrationRow;
use crate::models::table_id::TableId;
use crate::schema::*;
use crate::snapshots::{Introspectable, Snapshot, SnapshotProvider};

pub type TxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[enum_dispatch::enum_dispatch]
pub enum Engine {
    Postgres,
    Sqlite,
    Mysql,
    Detached,
}

impl Engine {
    pub fn detached(dialect: SqlDialect, table: TableId) -> Self {
        Engine::Detached(Detached::new(dialect, table))
    }

    pub async fn from_config(config: &Config) -> Result<Self> {
        let table: TableId = config.migrations.table.clone().into();

        if config.database_url.is_none() {
            // If no database URL is provided, use the detached engine which doesn't require a connection
            return Ok(Engine::Detached(Detached::new(config.dialect, table)));
        }

        match config.dialect {
            SqlDialect::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url.as_ref().ok_or_else(|| {
                        crate::ShkiError::migration("Database URL is required for Postgres engine")
                    })?)
                    .await?;
                Ok(Engine::Postgres(Postgres::new(pool, table)))
            }
            SqlDialect::Sqlite => {
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url.as_ref().ok_or_else(|| {
                        crate::ShkiError::migration("Database URL is required for Sqlite engine")
                    })?)
                    .await?;
                Ok(Engine::Sqlite(Sqlite::new(pool, table)))
            }
            SqlDialect::Mysql => {
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url.as_ref().ok_or_else(|| {
                        crate::ShkiError::migration("Database URL is required for MySQL engine")
                    })?)
                    .await?;
                Ok(Engine::Mysql(Mysql::new(pool, table)))
            }
        }
    }

    pub fn with_table(self, table: TableId) -> Self {
        match self {
            Engine::Postgres(engine) => Engine::Postgres(engine.with_table(table)),
            Engine::Sqlite(engine) => Engine::Sqlite(engine.with_table(table)),
            Engine::Mysql(engine) => Engine::Mysql(engine.with_table(table)),
            Engine::Detached(engine) => Engine::Detached(engine.with_table(table)),
        }
    }

    pub fn table(&self) -> &TableId {
        match self {
            Engine::Postgres(engine) => engine.table(),
            Engine::Sqlite(engine) => engine.table(),
            Engine::Mysql(engine) => engine.table(),
            Engine::Detached(engine) => engine.table(),
        }
    }
}

#[enum_dispatch::enum_dispatch(Engine)]
pub(crate) trait EngineDriver {
    /// make sure migrations table exists, if not create it
    async fn ensure_migrations(&self) -> Result<()>;

    /// Get all migration rows in the table sorted by applied_at ascending
    async fn select_migrations(&self) -> Result<Vec<MigrationRow>>;

    /// Apply a single migration within a transaction
    async fn apply_migration(&self, path: &Path) -> Result<MigrationRow>;

    /// Rollback a single migration using its down migration file
    ///
    /// Executes the down migration within a transaction and removes
    /// the migration record from the migrations table.
    async fn rollback_migration(&self, path: &Path) -> Result<()>;

    /// Mark a migration as applied without running it (used for baseline)
    async fn mark_applied(&self, path: &Path) -> Result<MigrationRow>;

    /// Delete all records from the migrations table
    async fn delete_table(&self) -> Result<()>;

    // /// Delete a single migration
    // async fn delete_migration(&self, name: &str) -> Result<MigrationRow>;
}

#[async_trait::async_trait]
impl<E> Introspectable for E
where
    E: EngineDriver + SnapshotProvider + Send + Sync,
{
    async fn introspect(&self, config: &Config) -> Result<Snapshot> {
        let mut snapshot = Snapshot::new(config.dialect);

        snapshot.enums = self.get_enums(&config.migrations.table.schema).await?;
        snapshot.views = self.get_views(&config.migrations.table.schema).await?;
        snapshot.sequences = self.get_sequences(&config.migrations.table.schema).await?;
        snapshot.extensions = self.get_extensions(&config.migrations.table.schema).await?;

        let mut tables = self.get_tables(&config.migrations.table.schema).await?;
        let constraints = self
            .get_constraints(&config.migrations.table.schema)
            .await?;

        dbg!("Got tables: {:#?}", &tables.iter().len());

        let columns = self.get_columns(&config.migrations.table.schema).await?;

        columns.into_iter().for_each(|(table_id, cols)| {
            if let Some(table) = tables.get_mut(&table_id) {
                table.columns = cols;
            }
        });

        constraints.into_iter().for_each(|(table_id, cons)| {
            if let Some(table) = tables.get_mut(&table_id) {
                table.constraints = cons;
            }
        });

        snapshot.tables = tables;

        Ok(snapshot)
    }
}
