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
use crate::migrate::checksum::sql_checksum;
use crate::migrate::directives::Directives;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::{split_statements, truncate_sql};
use crate::models::iden::Iden;
use crate::schema::*;
use crate::snapshots::SnapshotProvider;
use crate::{Result, ShkiError};

pub type TxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub(crate) struct MigrationFile {
    pub filename: String,
    pub name: String,
    pub sql: String,
    pub checksum: String,
    pub is_down: bool,
    pub directives: Directives,
}

/// Execute a no-transaction migration segment by segment, directly on the pool.
///
/// A failure mid-file leaves earlier segments committed and the migration
/// unrecorded, so re-running replays it from the top: no-transaction migrations
/// must be idempotent (`CREATE INDEX IF NOT EXISTS` and friends).
async fn execute_no_tx<DB>(pool: &sqlx::Pool<DB>, file: &MigrationFile) -> Result<()>
where
    DB: sqlx::Database,
    for<'c> &'c sqlx::Pool<DB>: sqlx::Executor<'c, Database = DB>,
{
    for segment in split_statements(&file.sql) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(segment.clone()))
            .execute(pool)
            .await
            .map_err(|e| {
                let mut message = format!(
                    "Failed to execute statement in no-transaction migration '{}': {}\nStatement: {}",
                    file.name,
                    e,
                    truncate_sql(&segment, 200)
                );
                if segment.to_uppercase().contains("CONCURRENTLY") {
                    message.push_str(
                        "\nHint: a failed concurrent index build can leave an INVALID index behind; \
                         check pg_index.indisvalid, DROP INDEX it, then re-run.",
                    );
                }
                ShkiError::migration(message)
            })?;
    }
    Ok(())
}

/// Apply a no-transaction migration: execute its segments outside any
/// transaction, then record it in the journal table.
async fn apply_no_tx<E>(
    engine: &E,
    pool: &sqlx::Pool<E::Database>,
    file: &MigrationFile,
) -> Result<MigrationRow>
where
    E: TransactionalEngine,
    for<'c> &'c sqlx::Pool<E::Database>: sqlx::Executor<'c, Database = E::Database>,
{
    with_tx!(pool, |tx| { engine.ensure_migrations(&mut tx).await })?;
    execute_no_tx(pool, file).await?;
    with_tx!(pool, |tx| { engine.insert_migration(&mut tx, file).await })
}

/// Roll back a no-transaction migration: execute its down segments outside any
/// transaction, then delete the journal row.
async fn rollback_no_tx<E>(
    engine: &E,
    pool: &sqlx::Pool<E::Database>,
    file: &MigrationFile,
) -> Result<()>
where
    E: TransactionalEngine,
    for<'c> &'c sqlx::Pool<E::Database>: sqlx::Executor<'c, Database = E::Database>,
{
    if !file.is_down {
        return Err(ShkiError::migration(
            "Down migration must end with .down.sql",
        ));
    }
    execute_no_tx(pool, file).await?;
    with_tx!(pool, |tx| {
        engine.delete_migration(&mut tx, &file.name).await
    })?;
    Ok(())
}

pub(crate) trait TransactionalEngine {
    type Database: sqlx::Database;

    async fn ensure_migrations(&self, tx: &mut sqlx::Transaction<'_, Self::Database>)
    -> Result<()>;

    async fn apply_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationFile>;

    async fn insert_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        file: &MigrationFile,
    ) -> Result<MigrationRow>;

    async fn rollback_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<()>;

    async fn mark_applied(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationRow>;

    async fn delete_table(&self, tx: &mut sqlx::Transaction<'_, Self::Database>) -> Result<()>;

    async fn delete_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        name: &str,
    ) -> Result<MigrationRow>;

    async fn migrations_table_exists(&self) -> Result<bool>;

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>>;

    fn read_migration_file(&self, path: &Path) -> Result<MigrationFile> {
        let sql = std::fs::read_to_string(path)?;
        let name = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            ShkiError::migration(format!(
                "Invalid migration path: {}",
                path.to_string_lossy()
            ))
        })?;
        let filename = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
            ShkiError::migration(format!(
                "Invalid migration filename: {}",
                path.to_string_lossy()
            ))
        })?;
        let (name, is_down) = match name.strip_suffix(".down") {
            Some(name) => (name, true),
            None => (name, false),
        };
        let checksum = sql_checksum(&sql);
        let directives = Directives::parse(&sql)?;
        Ok(MigrationFile {
            filename: filename.to_string(),
            name: name.to_string(),
            sql,
            checksum,
            is_down,
            directives,
        })
    }
}

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

        if config.database_url().is_none() {
            // If no database URL is provided, use the detached engine which doesn't require a connection
            return Ok(Engine::Detached(Detached::new(config.dialect(), table)));
        }

        match config.dialect() {
            SqlDialect::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5) // TODO: make this configurable?
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url().ok_or_else(|| {
                        crate::ShkiError::migration("Database URL is required for Postgres engine")
                    })?)
                    .await?;
                Ok(Engine::Postgres(Postgres::new(pool, table)))
            }
            SqlDialect::Sqlite => {
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url().ok_or_else(|| {
                        crate::ShkiError::migration("Database URL is required for Sqlite engine")
                    })?)
                    .await?;
                Ok(Engine::Sqlite(Sqlite::new(pool, table)))
            }
            SqlDialect::Mysql => {
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
                    .connect(config.database_url().ok_or_else(|| {
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
                engine.ensure_migrations(&mut tx).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations(&mut tx).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations(&mut tx).await
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
        macro_rules! apply {
            ($engine:expr) => {{
                let file = $engine.read_migration_file(path)?;
                if file.directives.no_transaction {
                    apply_no_tx($engine, &$engine.pool, &file).await
                } else {
                    with_tx!($engine.pool, |tx| {
                        $engine.ensure_migrations(&mut tx).await?;
                        let applied = $engine.apply_migration(&mut tx, path).await?;
                        $engine.insert_migration(&mut tx, &applied).await
                    })
                }
            }};
        }

        match self {
            Engine::Postgres(engine) => apply!(engine),
            Engine::Sqlite(engine) => apply!(engine),
            Engine::Mysql(engine) => apply!(engine),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn rollback_migration(&self, path: &Path) -> Result<()> {
        macro_rules! rollback {
            ($engine:expr) => {{
                let file = $engine.read_migration_file(path)?;
                if file.directives.no_transaction {
                    rollback_no_tx($engine, &$engine.pool, &file).await
                } else {
                    with_tx!($engine.pool, |tx| {
                        $engine.rollback_migration(&mut tx, path).await
                    })
                }
            }};
        }

        match self {
            Engine::Postgres(engine) => rollback!(engine),
            Engine::Sqlite(engine) => rollback!(engine),
            Engine::Mysql(engine) => rollback!(engine),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn mark_applied(&self, path: &Path) -> Result<MigrationRow> {
        match self {
            Engine::Postgres(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations(&mut tx).await?;
                engine.mark_applied(&mut tx, path).await
            }),
            Engine::Sqlite(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations(&mut tx).await?;
                engine.mark_applied(&mut tx, path).await
            }),
            Engine::Mysql(engine) => with_tx!(engine.pool, |tx| {
                engine.ensure_migrations(&mut tx).await?;
                engine.mark_applied(&mut tx, path).await
            }),
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }

    pub(crate) async fn delete_table(&self) -> Result<()> {
        match self {
            Engine::Postgres(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table(&mut tx).await })
            }
            Engine::Sqlite(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table(&mut tx).await })
            }
            Engine::Mysql(engine) => {
                with_tx!(engine.pool, |tx| { engine.delete_table(&mut tx).await })
            }
            Engine::Detached(engine) => Err(engine.unavailable()),
        }
    }
}
