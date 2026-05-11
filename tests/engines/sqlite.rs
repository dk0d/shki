use sqlx::Pool;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use tempfile::TempDir;

use super::TestBackend;
use shki::engines::Engine;
use shki::engines::sqlite::Sqlite;
use shki::migrate::manager::MigrationManager;
use shki::models::table_id::TableId;
use shki::schema::SqlDialect;

use crate::unique_suffix;

pub struct SqliteTestContext {
    pub temp_dir: TempDir,
    pub db_path: PathBuf,
    pub pool: Pool<sqlx::Sqlite>,
    pub migrations_dir: PathBuf,
    pub suffix: String,
}

impl SqliteTestContext {
    pub fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join(format!("{name}.db"));
        let migrations_dir = temp_dir.path().join("migrations");

        std::fs::File::create(&db_path).expect("failed to create sqlite db file");
        std::fs::create_dir_all(&migrations_dir).expect("failed to create migrations dir");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy(&format!("sqlite://{}", db_path.display()))
            .expect("failed to create sqlite pool");

        Self {
            temp_dir,
            db_path,
            pool,
            migrations_dir,
            suffix: unique_suffix(),
        }
    }

    pub fn url(&self) -> String {
        format!("sqlite://{}", self.db_path.display())
    }
}

impl TestBackend for SqliteTestContext {
    async fn setup(name: &str) -> Self {
        Self::new(name)
    }

    fn dialect(&self) -> SqlDialect {
        SqlDialect::Sqlite
    }

    fn migrations_dir(&self) -> &std::path::Path {
        &self.migrations_dir
    }

    fn database_url(&self) -> String {
        self.url()
    }

    fn migration_schema(&self) -> Option<&str> {
        None
    }

    fn engine(&self, table: TableId) -> Engine {
        Engine::Sqlite(Sqlite::new(self.pool.clone(), table))
    }

    fn unique_name(&self, prefix: &str) -> String {
        format!("{}_{}", prefix, self.suffix)
    }

    fn text_type(&self) -> &'static str {
        "TEXT"
    }

    fn primary_key_type(&self) -> &'static str {
        "INTEGER PRIMARY KEY"
    }

    fn root_dir(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    async fn table_exists(&self, table_name: &str) -> bool {
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table_name)
            .fetch_optional(&self.pool)
            .await
            .expect("failed to query sqlite_master")
            .is_some()
    }

    async fn migration_table_exists(&self, manager: &MigrationManager) -> bool {
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(manager.table.name.as_str())
            .fetch_optional(&self.pool)
            .await
            .expect("failed to query sqlite_master")
            .is_some()
    }

    async fn cleanup(self) {}
}
