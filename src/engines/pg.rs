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

    pub(crate) async fn ensure_migrations_in(
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

    pub(crate) async fn mark_applied_in<'e, E>(&self, exec: E, path: &Path) -> Result<MigrationRow>
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

    pub(crate) async fn rollback_migration_in(
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

    pub(crate) async fn apply_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        path: &Path,
    ) -> Result<MigrationRow> {
        let sql = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;
        let checksum = sql_checksum(&sql);

        sqlx::raw_sql(&sql).execute(&mut **tx).await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to execute statement in migration '{}': {}\nStatement: {}",
                name,
                e,
                truncate_sql(&sql, 200)
            ))
        })?;

        self.insert_migration_in(&mut **tx, name, &checksum).await
    }

    pub(crate) async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
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

    pub(crate) async fn migrations_table_exists(&self) -> Result<bool> {
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

    pub(crate) async fn delete_table_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        let table_name =
            SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(&self.migrations_table);
        let _ = sqlx::query(&format!("DROP TABLE {}", table_name))
            .execute(&mut **tx)
            .await;
        Ok(())
    }
}
