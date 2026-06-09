use sqlx::Pool;

use crate::engines::TransactionalEngine;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::generator::SqlGenerator;
use crate::{Result, ShkiError};
use std::path::Path;

use super::MigrationFile;

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
}

impl TransactionalEngine for Sqlite {
    type Database = sqlx::Sqlite;

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

    async fn ensure_migrations(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
    ) -> Result<()> {
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
        sqlx::query(&query)
            .execute(&mut **tx)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
        Ok(())
    }

    async fn apply_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationFile> {
        let summary = self.read_migration_file(path)?;

        sqlx::raw_sql(&summary.sql)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in migration '{}': {}\nStatement: {}",
                    summary.name,
                    e,
                    truncate_sql(&summary.sql, 200)
                ))
            })?;

        Ok(summary)
    }

    async fn insert_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        applied: &MigrationFile,
    ) -> Result<MigrationRow> {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            "INSERT INTO {} (name, checksum) VALUES (?, ?) RETURNING id, name, checksum, applied_at",
            table_name
        );
        sqlx::query_as::<_, MigrationRow>(&query)
            .bind(&applied.name)
            .bind(&applied.checksum)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to record migration '{}': {}",
                    applied.name, e
                ))
            })
    }

    async fn rollback_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<()> {
        let summary = self.read_migration_file(path)?;

        sqlx::raw_sql(&summary.sql)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in down migration '{}': {}\nStatement: {}",
                    summary.name,
                    e,
                    truncate_sql(&summary.sql, 200)
                ))
            })?;

        let _ = self.delete_migration(tx, &summary.name).await?;
        Ok(())
    }

    async fn mark_applied(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationRow> {
        let summary = self.read_migration_file(path)?;
        self.insert_migration(tx, &summary).await
    }

    async fn delete_table(&self, tx: &mut sqlx::Transaction<'_, Self::Database>) -> Result<()> {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!("DROP TABLE {}", table_name);
        sqlx::query(&query)
            .execute(&mut **tx)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
        Ok(())
    }

    async fn delete_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        name: &str,
    ) -> Result<MigrationRow> {
        let table_name = SqlGenerator::new(&SqlDialect::Sqlite).qualified_table_name(&self.table);
        let query = format!(
            "DELETE FROM {} WHERE name = ? RETURNING id, name, checksum, applied_at",
            table_name
        );
        sqlx::query_as::<_, MigrationRow>(&query)
            .bind(name)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to delete migration '{}': {}", name, e))
            })
    }
}
