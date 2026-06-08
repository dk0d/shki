use sqlx::{Pool, SqliteExecutor};

use super::EngineDriver;
use crate::engines::utils::tx::with_tx;
use crate::migrate::checksum::sql_checksum;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::generator::SqlGenerator;
use crate::{Result, ShkiError};
use std::path::Path;

pub struct Sqlite {
    table: Iden,
    pub(crate) pool: Pool<sqlx::Sqlite>,
}

impl Sqlite {
    pub fn new(pool: Pool<sqlx::Sqlite>, table: Iden) -> Self {
        Self { pool, table }
    }

    pub fn with_table(mut self, table: Iden) -> Self {
        self.table = table;
        self
    }

    pub fn table(&self) -> &Iden {
        &self.table
    }

    async fn execute_query<'e, E>(&self, exec: E, query: String) -> Result<()>
    where
        E: SqliteExecutor<'e>,
    {
        sqlx::query(&query)
            .execute(exec)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
        Ok(())
    }

    async fn ensure_migrations_in<'e, E>(&self, exec: E) -> Result<()>
    where
        E: SqliteExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    checksum TEXT,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        );
        self.execute_query(exec, query).await
    }

    async fn insert_migration_in<'e, E>(
        &self,
        exec: E,
        name: &str,
        checksum: &str,
    ) -> Result<MigrationRow>
    where
        E: SqliteExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            "INSERT INTO {} (name, checksum) VALUES (?, ?) RETURNING id, name, checksum, applied_at",
            table_name
        );
        sqlx::query_as::<_, MigrationRow>(&query)
            .bind(name)
            .bind(checksum)
            .fetch_one(exec)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to record migration '{}': {}", name, e))
            })
    }

    async fn mark_applied_in<'e, E>(&self, exec: E, path: &Path) -> Result<MigrationRow>
    where
        E: SqliteExecutor<'e>,
    {
        let sql = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;
        let checksum = sql_checksum(&sql);
        self.insert_migration_in(exec, name, &checksum).await
    }

    async fn delete_migration_in<'e, E>(&self, exec: E, name: &str) -> Result<MigrationRow>
    where
        E: SqliteExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            "DELETE FROM {} WHERE name = ? RETURNING id, name, checksum, applied_at",
            table_name
        );
        sqlx::query_as::<_, MigrationRow>(&query)
            .bind(name)
            .fetch_one(exec)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to delete migration '{}': {}", name, e))
            })
    }

    async fn rollback_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        path: &Path,
    ) -> Result<()> {
        let sql = std::fs::read_to_string(path)?;
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid down migration filename"))?;

        let name = filename
            .strip_suffix(".down.sql")
            .ok_or_else(|| ShkiError::migration("Down migration must end with .down.sql"))?;

        sqlx::raw_sql(&sql).execute(&mut **tx).await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to execute statement in down migration '{}': {}\nStatement: {}",
                name,
                e,
                truncate_sql(&sql, 200)
            ))
        })?;

        let _ = self.delete_migration_in(&mut **tx, name).await?;
        Ok(())
    }
}

impl EngineDriver for Sqlite {
    async fn ensure_migrations(&self) -> Result<()> {
        with_tx!(self.pool, |tx| {
            self.ensure_migrations_in(&mut *tx).await
        })
    }

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        );

        let rows = sqlx::query_as::<_, MigrationRow>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn migrations_table_exists(&self) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
        )
        .bind(&self.table.name)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    async fn apply_migration(&self, path: &Path) -> Result<MigrationRow> {
        with_tx!(self.pool, |tx| {
            self.ensure_migrations_in(&mut *tx).await?;

            let sql = std::fs::read_to_string(path)?;
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;
            let checksum = sql_checksum(&sql);

            sqlx::raw_sql(&sql).execute(&mut *tx).await.map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in migration '{}': {}\nStatement: {}",
                    name,
                    e,
                    truncate_sql(&sql, 200)
                ))
            })?;

            self.insert_migration_in(&mut *tx, name, &checksum).await
        })
    }

    async fn rollback_migration(&self, path: &Path) -> Result<()> {
        with_tx!(self.pool, |tx| {
            self.rollback_migration_in(&mut tx, path).await
        })
    }

    async fn mark_applied(&self, path: &Path) -> Result<MigrationRow> {
        with_tx!(self.pool, |tx| {
            self.ensure_migrations_in(&mut *tx).await?;
            self.mark_applied_in(&mut *tx, path).await
        })
    }

    async fn delete_table(&self) -> Result<()> {
        with_tx!(self.pool, |tx| {
            let table_name =
                SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
            let query = format!("DROP TABLE {}", table_name);
            sqlx::query(&query)
                .execute(&mut *tx)
                .await
                .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
            Ok(())
        })
    }
}
