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

use crate::config::Config;
use crate::engines::utils::tx::with_tx;
use crate::migrate::manager::MigrationRow;
use crate::models::iden::Iden;
use crate::schema::*;
use crate::snapshots::SnapshotProvider;
use crate::{Result, ShkiError};

pub type TxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[enum_dispatch::enum_dispatch]
pub enum Engine {
    Postgres(Postgres),
    Sqlite(Sqlite),
    Mysql(Mysql),
    Detached(Detached),
}

impl Engine {
    pub fn detached(dialect: SqlDialect, table: Iden) -> Self {
        Engine::Detached(Detached::new(dialect, table))
    }

    pub async fn from_config(config: &Config) -> Result<Self> {
        let table: Iden = config.migrations.entity().clone();

        if config.database_url.is_none() {
            // If no database URL is provided, use the detached engine which doesn't require a connection
            return Ok(Engine::Detached(Detached::new(config.dialect, table)));
        }

        match config.dialect {
            SqlDialect::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5) // TODO: make this configurable?
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

    pub fn with_table(self, table: Iden) -> Self {
        match self {
            Engine::Postgres(engine) => Engine::Postgres(engine.with_table(table)),
            Engine::Sqlite(engine) => Engine::Sqlite(engine.with_table(table)),
            Engine::Mysql(engine) => Engine::Mysql(engine.with_table(table)),
            Engine::Detached(engine) => Engine::Detached(engine.with_table(table)),
        }
    }

    pub fn table(&self) -> &Iden {
        match self {
            Engine::Postgres(engine) => engine.table(),
            Engine::Sqlite(engine) => engine.table(),
            Engine::Mysql(engine) => engine.table(),
            Engine::Detached(engine) => engine.table(),
        }
    }

    pub(crate) async fn ensure_migrations(&self) -> Result<()> {
        match self {
            Engine::Postgres(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut tx).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await
            }),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        match self {
            Engine::Postgres(engine) => engine.select_migrations().await,
            Engine::Sqlite(engine) => engine.select_migrations().await,
            Engine::Mysql(engine) => engine.select_migrations().await,
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn migrations_table_exists(&self) -> Result<bool> {
        match self {
            Engine::Postgres(engine) => engine.migrations_table_exists().await,
            Engine::Sqlite(engine) => engine.migrations_table_exists().await,
            Engine::Mysql(engine) => engine.migrations_table_exists().await,
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn apply_migration(&self, path: &Path) -> Result<MigrationRow> {
        match self {
            Engine::Postgres(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut tx).await?;
                engine.apply_migration_in(&mut tx, path).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await?;
                engine.apply_migration_in(&mut tx, path).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await?;
                engine.apply_migration_in(&mut tx, path).await
            }),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn rollback_migration(&self, path: &Path) -> Result<()> {
        match self {
            Engine::Postgres(engine) => with_tx!(engine.pool, |tx| {
                engine.rollback_migration_in(&mut tx, path).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.rollback_migration_in(&mut tx, path).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.rollback_migration_in(&mut tx, path).await
            }),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn mark_applied(&self, path: &Path) -> Result<MigrationRow> {
        match self {
            Engine::Postgres(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut tx).await?;
                engine.mark_applied_in(&mut *tx, path).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await?;
                engine.mark_applied_in(&mut *tx, path).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations_in(&mut *tx).await?;
                engine.mark_applied_in(&mut tx, path).await
            }),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn delete_table(&self) -> Result<()> {
        match self {
            Engine::Postgres(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table_in(&mut tx).await })
            }
            Engine::Sqlite(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table_in(&mut *tx).await })
            }
            Engine::Mysql(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table_in(&mut *tx).await })
            }
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }
}
