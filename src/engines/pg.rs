use crate::engines::TransactionalEngine;

use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::render::SqlRenderer;
use crate::{Result, ShkiError};
use sqlx::{AssertSqlSafe, Pool};
use std::path::Path;

use super::MigrationFile;

pub struct Postgres {
    migrations_table: Iden,
    pub(crate) pool: Pool<sqlx::Postgres>,
}

impl Postgres {
    pub fn new(pool: Pool<sqlx::Postgres>, migrations_table: Iden) -> Self {
        Self {
            pool,
            migrations_table,
        }
    }

    pub fn with_table(mut self, table: Iden) -> Self {
        self.migrations_table = table;
        self
    }

    pub fn table(&self) -> &Iden {
        &self.migrations_table
    }

    fn insert_migration_query(&self, table: &Iden) -> String {
        let table_name = SqlRenderer::new(&SqlDialect::Postgres).qualified_table_name(table);
        format!(
            "INSERT INTO {} (name, checksum) VALUES ($1, $2) returning id, name, checksum, applied_at",
            table_name
        )
    }
}

impl TransactionalEngine for Postgres {
    type Database = sqlx::Postgres;

    async fn ensure_migrations(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
    ) -> Result<()> {
        let renderer = SqlRenderer::new(&SqlDialect::Postgres);
        let schema_name =
            renderer.quote_identifier(self.migrations_table.schema.as_deref().unwrap_or("public"));
        let table_name = renderer.qualified_table_name(&self.migrations_table);
        let query = format!(
            r#"
                CREATE SCHEMA IF NOT EXISTS {};
                CREATE TABLE IF NOT EXISTS {} (
                    id BIGSERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
            "#,
            schema_name, table_name
        );
        sqlx::raw_sql(AssertSqlSafe(query))
            .execute(&mut **tx)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}",)))?;
        Ok(())
    }

    async fn apply_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationFile> {
        let file = self.read_migration_file(path)?;
        sqlx::raw_sql(AssertSqlSafe(file.sql.clone()))
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                let mut message = format!(
                    "Failed to execute statement in migration '{}': {}\nStatement: {}",
                    file.name,
                    e,
                    truncate_sql(&file.sql, 200)
                );
                if e.to_string()
                    .contains("cannot run inside a transaction block")
                {
                    message.push_str(
                        "\nHint: add '-- shki:no-transaction' to this migration to run it outside \
                         the wrapping transaction, one '--> +statement' segment at a time. Such \
                         migrations must be idempotent.",
                    );
                }
                ShkiError::migration(message)
            })?;

        Ok(file)
    }

    async fn rollback_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<()> {
        let file = self.read_migration_file(path)?;

        if !file.is_down {
            return Err(ShkiError::migration(
                "Down migration must end with .down.sql",
            ));
        }

        sqlx::raw_sql(AssertSqlSafe(file.sql.clone()))
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in down migration '{}': {}\nStatement: {}",
                    file.name,
                    e,
                    truncate_sql(&file.sql, 200)
                ))
            })?;

        let _ = self.delete_migration(tx, &file.name).await?;

        Ok(())
    }

    async fn mark_applied(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationRow> {
        let file = self.read_migration_file(path)?;
        self.insert_migration(tx, &file).await
    }

    async fn insert_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        applied: &MigrationFile,
    ) -> Result<MigrationRow> {
        let query = self.insert_migration_query(&self.migrations_table);
        sqlx::query_as::<_, MigrationRow>(AssertSqlSafe(query))
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

    async fn delete_table(&self, tx: &mut sqlx::Transaction<'_, Self::Database>) -> Result<()> {
        let table_name =
            SqlRenderer::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
        let query = format!("DROP TABLE {}", table_name);
        let _ = sqlx::raw_sql(AssertSqlSafe(query)).execute(&mut **tx).await;
        Ok(())
    }

    async fn delete_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        name: &str,
    ) -> Result<MigrationRow> {
        let table_name =
            SqlRenderer::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
        let query = format!("DELETE FROM {} WHERE name = $1 returning *", table_name);
        let row = sqlx::query_as::<_, MigrationRow>(AssertSqlSafe(query))
            .bind(name)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}",)))?;
        Ok(row)
    }

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        let table_name =
            SqlRenderer::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
        let query = format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        );
        let rows = sqlx::query_as::<_, MigrationRow>(AssertSqlSafe(query))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn migrations_table_exists(&self) -> Result<bool> {
        let schema = self.migrations_table.schema.as_deref().unwrap_or("public");
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = $1 AND table_name = $2)",
        )
        .bind(schema)
        .bind(&self.migrations_table.name)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }
}
