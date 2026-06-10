use sqlx::{AssertSqlSafe, Pool};

use crate::engines::TransactionalEngine;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::render::SqlRenderer;
use crate::{Result, ShkiError};
use std::path::Path;

use super::MigrationFile;

pub struct Mysql {
    table: Iden,
    pub(crate) pool: Pool<sqlx::MySql>,
}

impl Mysql {
    pub fn new(pool: Pool<sqlx::MySql>, table: Iden) -> Self {
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

impl TransactionalEngine for Mysql {
    type Database = sqlx::MySql;

    async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let query = format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at from {} ORDER BY id",
            table_name
        );
        let rows = sqlx::query_as::<_, MigrationRow>(AssertSqlSafe(query))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn migrations_table_exists(&self) -> Result<bool> {
        let table_schema = match self.table.schema.as_deref() {
            Some(schema) => schema.to_string(),
            None => sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                .fetch_one(&self.pool)
                .await?
                .ok_or_else(|| ShkiError::migration("No MySQL database selected"))?,
        };
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = ? AND table_name = ?)",
        )
        .bind(&table_schema)
        .bind(&self.table.name)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    async fn ensure_migrations(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
    ) -> Result<()> {
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let query = format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        );
        sqlx::query(AssertSqlSafe(query))
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
        let applied = self.read_migration_file(path)?;

        sqlx::raw_sql(AssertSqlSafe(applied.sql.clone()))
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in migration '{}': {}\nStatement: {}",
                    applied.name,
                    e,
                    truncate_sql(&applied.sql, 200)
                ))
            })?;

        Ok(applied)
    }

    async fn insert_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        applied: &MigrationFile,
    ) -> Result<MigrationRow> {
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let insert = AssertSqlSafe(format!(
            "INSERT INTO {} (name, checksum) VALUES (?, ?)",
            table_name
        ));
        sqlx::query(insert)
            .bind(&applied.name)
            .bind(&applied.checksum)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to record migration '{}': {}",
                    applied.name, e
                ))
            })?;

        let select = AssertSqlSafe(format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at FROM {} WHERE name = ?",
            table_name
        ));
        sqlx::query_as::<_, MigrationRow>(select)
            .bind(&applied.name)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to load migration '{}': {}",
                    applied.name, e
                ))
            })
    }

    async fn rollback_migration(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<()> {
        let applied = self.read_migration_file(path)?;
        if !applied.is_down {
            return Err(ShkiError::migration(
                "Down migration must end with .down.sql",
            ));
        };
        sqlx::raw_sql(AssertSqlSafe(applied.sql.clone()))
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to execute statement in down migration '{}': {}\nStatement: {}",
                    applied.name,
                    e,
                    truncate_sql(&applied.sql, 200)
                ))
            })?;

        let _ = self.delete_migration(tx, &applied.name).await?;
        Ok(())
    }

    async fn mark_applied(
        &self,
        tx: &mut sqlx::Transaction<'_, Self::Database>,
        path: &Path,
    ) -> Result<MigrationRow> {
        let applied = self.read_migration_file(path)?;
        self.insert_migration(tx, &applied).await
    }

    async fn delete_table(&self, tx: &mut sqlx::Transaction<'_, Self::Database>) -> Result<()> {
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let query = AssertSqlSafe(format!("DROP TABLE {}", table_name));
        sqlx::query(query)
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
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let select = format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at FROM {} WHERE name = ?",
            table_name
        );
        let row = sqlx::query_as::<_, MigrationRow>(AssertSqlSafe(select))
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to load migration '{}': {}", name, e))
            })?;

        let delete = AssertSqlSafe(format!("DELETE FROM {} WHERE name = ?", table_name));
        sqlx::query(delete)
            .bind(name)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to delete migration '{}': {}", name, e))
            })?;

        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::queries;

    fn table() -> Iden {
        Iden::new("__shki_migrations", Some("meta".to_string()))
    }

    #[tokio::test]
    async fn with_table_replaces_engine_table() {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://root:password@localhost/shki")
            .expect("failed to create lazy mysql pool");
        let engine = Mysql::new(pool, Iden::new("old_table", None)).with_table(table());

        assert_eq!(engine.table(), &table());
    }

    #[test]
    fn mysql_queries_use_mysql_placeholders_and_identifiers() {
        let table = table();

        assert!(
            queries::ensure_migrations(&SqlDialect::Mysql, &table)
                .contains("CREATE TABLE IF NOT EXISTS `meta`.`__shki_migrations`")
        );
        assert!(queries::insert_migration(&SqlDialect::Mysql, &table).contains("VALUES (?, ?)"));
        assert!(queries::select_migrations(&SqlDialect::Mysql, &table).contains("ORDER BY id"));
    }

    #[test]
    fn mysql_qualified_table_name_uses_backticks() {
        let table_name = SqlRenderer::new(&SqlDialect::Mysql).qualified_table_name(&table());

        assert_eq!(table_name, "`meta`.`__shki_migrations`");
    }
}
