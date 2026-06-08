use crate::engines::EngineDriver;
use crate::engines::utils::tx::with_tx;
use crate::migrate::checksum::sql_checksum;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::generator::SqlGenerator;
use crate::{Result, ShkiError};
use sqlx::{PgExecutor, Pool};
use std::path::Path;

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
        let table_name = SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(table);
        format!(
            "INSERT INTO {} (name, checksum) VALUES ($1, $2) returning id, name, checksum, applied_at",
            table_name
        )
    }

    async fn ensure_migrations_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        let table_name =
            SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);

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
            self.migrations_table
                .schema
                .clone()
                .unwrap_or("public".to_string()),
            table_name
        );
        sqlx::raw_sql(&query)
            .execute(&mut **tx)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}",)))?;
        Ok(())
    }

    async fn insert_migration_in<'e, E>(
        &self,
        exec: E,
        name: &str,
        checksum: &str,
    ) -> Result<MigrationRow>
    where
        E: PgExecutor<'e>,
    {
        let query = self.insert_migration_query(&self.migrations_table);
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
        E: PgExecutor<'e>,
    {
        let sql = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;
        let checksum = sql_checksum(&sql);
        self.insert_migration_in(exec, name, &checksum).await
    }

    async fn rollback_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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

    async fn delete_migration_in<'s, 'e, E>(&self, exec: E, name: &'s str) -> Result<MigrationRow>
    where
        E: PgExecutor<'e>,
    {
        let table_name =
            SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
        let query = format!("DELETE FROM {} WHERE name = $1 returning *", table_name);
        let row = sqlx::query_as::<_, MigrationRow>(&query)
            .bind(name)
            .fetch_one(exec)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}",)))?;
        Ok(row)
    }
}

impl EngineDriver for Postgres {
    async fn ensure_migrations(&self) -> Result<()> {
        with_tx!(self.pool, |tx| { self.ensure_migrations_in(&mut tx).await })
    }

    /// Record an existing migration file as applied without executing its SQL.
    ///
    /// This is useful for bootstrap/adoption workflows where the database already
    /// matches the migration state and only tracking metadata should be inserted.
    async fn mark_applied(&self, path: &Path) -> Result<MigrationRow> {
        with_tx!(self.pool, |tx| {
            self.ensure_migrations_in(&mut tx).await?;
            self.mark_applied_in(&mut *tx, path).await
        })
    }

    /// Rollback a single migration using its down migration file
    ///
    /// Executes the down migration within a transaction and removes
    /// the migration record from the migrations table.
    async fn rollback_migration(&self, path: &Path) -> Result<()> {
        with_tx!(self.pool, |tx| {
            self.rollback_migration_in(&mut tx, path).await
        })
    }

    /// Apply a single migration within a transaction
    ///
    /// The entire migration (all statements) is executed within a single transaction.
    /// If any statement fails, the entire migration is rolled back.
    ///
    /// Note: Some statements like `CREATE INDEX CONCURRENTLY` in PostgreSQL cannot
    /// run inside a transaction. For such cases, use separate migration files.
    async fn apply_migration(&self, path: &Path) -> Result<MigrationRow> {
        with_tx!(self.pool, |tx| {
            self.ensure_migrations_in(&mut tx).await?;

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

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        let table_name =
            SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
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

    async fn delete_table(&self) -> Result<()> {
        with_tx!(self.pool, |tx| {
            let table_name = SqlGenerator::new(&SqlDialect::Postgres)
                .qualified_table_name(&self.migrations_table);
            let query = format!("DROP TABLE {}", table_name);
            let _ = sqlx::query(&query).execute(&mut *tx).await;
            Ok(())
        })
    }
}
