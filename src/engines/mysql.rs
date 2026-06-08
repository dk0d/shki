use sqlx::{MySqlExecutor, Pool};

use crate::migrate::checksum::sql_checksum;
use crate::migrate::manager::MigrationRow;
use crate::migrate::utils::truncate_sql;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;
use crate::sql::generator::SqlGenerator;
use crate::{Result, ShkiError};
use std::path::Path;

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

    pub(crate) async fn ensure_migrations_in<'e, E>(&self, exec: E) -> Result<()>
    where
        E: MySqlExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
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
        sqlx::query(&query)
            .execute(exec)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
        Ok(())
    }

    async fn insert_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        name: &str,
        checksum: &str,
    ) -> Result<MigrationRow> {
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let insert = format!("INSERT INTO {} (name, checksum) VALUES (?, ?)", table_name);
        sqlx::query(&insert)
            .bind(name)
            .bind(checksum)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to record migration '{}': {}", name, e))
            })?;

        let select = format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at FROM {} WHERE name = ?",
            table_name
        );
        sqlx::query_as::<_, MigrationRow>(&select)
            .bind(name)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to load migration '{}': {}", name, e))
            })
    }

    pub(crate) async fn mark_applied_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        path: &Path,
    ) -> Result<MigrationRow> {
        let sql = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;
        let checksum = sql_checksum(&sql);
        self.insert_migration_in(tx, name, &checksum).await
    }

    async fn delete_migration_in<'e, E>(&self, exec: E, name: &str) -> Result<MigrationRow>
    where
        E: MySqlExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let select = format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at FROM {} WHERE name = ?",
            table_name
        );
        let row = sqlx::query_as::<_, MigrationRow>(&select)
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to load migration '{}': {}", name, e))
            })?;

        let delete = format!("DELETE FROM {} WHERE name = ?", table_name);
        sqlx::query(&delete)
            .bind(name)
            .execute(exec)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to delete migration '{}': {}", name, e))
            })?;

        Ok(row)
    }

    pub(crate) async fn rollback_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
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

    pub(crate) async fn apply_migration_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
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

        self.insert_migration_in(tx, name, &checksum).await
    }

    pub(crate) async fn select_migrations(&self) -> Result<Vec<MigrationRow>> {
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let query = format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at from {} ORDER BY id",
            table_name
        );
        let rows = sqlx::query_as::<_, MigrationRow>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub(crate) async fn migrations_table_exists(&self) -> Result<bool> {
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

    pub(crate) async fn delete_table_in<'e, E>(&self, exec: E) -> Result<()>
    where
        E: MySqlExecutor<'e>,
    {
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&self.table);
        let query = format!("DROP TABLE {}", table_name);
        sqlx::query(&query)
            .execute(exec)
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to execute query {e}")))?;
        Ok(())
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
        let table_name = SqlGenerator::new(&SqlDialect::Mysql).qualified_table_name(&table());

        assert_eq!(table_name, "`meta`.`__shki_migrations`");
    }
}
