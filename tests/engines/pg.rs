use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, Pool, Postgres};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PostgresContainer;

use super::{TestBackend, cleanup_postgres_schema};
use shki::engines::Engine;
use shki::engines::pg::Postgres as PostgresEngine;
use shki::migrate::manager::MigrationManager;
use shki::models::table_id::TableId;
use shki::schema::SqlDialect;

use crate::common::{connect_with_retries, unique_suffix};

pub struct TestDatabase {
    pub _container: ContainerAsync<PostgresContainer>,
    pub database_url: String,
}

impl TestDatabase {
    pub async fn start() -> Self {
        let container = PostgresContainer::default()
            .with_db_name("postgres")
            .with_user("postgres")
            .with_password("postgres")
            .with_tag("16-alpine")
            .start()
            .await
            .expect("failed to start postgres test container");

        let host = container
            .get_host()
            .await
            .expect("failed to get postgres test container host");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("failed to get postgres test container port");

        Self {
            database_url: format!("postgresql://postgres:postgres@{host}:{port}/postgres"),
            _container: container,
        }
    }
}

pub struct PgTestContext {
    pub _database: TestDatabase,
    pub pg_pool: Pool<Postgres>,
    pub schema_name: String,
    pub temp_dir: TempDir,
    pub migrations_dir: PathBuf,
    pub suffix: String,
}

impl PgTestContext {
    pub async fn new(name: &str) -> Self {
        let database = TestDatabase::start().await;
        let url = database.database_url.clone();
        let pg_pool = connect_with_retries("Postgres", || {
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&url)
        })
        .await;
        let schema_name = format!("{}_{}", name, unique_suffix());
        pg_pool
            .execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
            .await
            .expect("failed to drop schema");
        pg_pool
            .execute(format!("CREATE SCHEMA \"{}\"", schema_name).as_str())
            .await
            .expect("failed to create schema");

        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir_all(&migrations_dir).expect("failed to create migrations dir");

        Self {
            _database: database,
            pg_pool,
            schema_name,
            temp_dir,
            migrations_dir,
            suffix: unique_suffix(),
        }
    }
}

impl TestBackend for PgTestContext {
    async fn setup(name: &str) -> Self {
        Self::new(name).await
    }

    fn dialect(&self) -> SqlDialect {
        SqlDialect::Postgres
    }

    fn migrations_dir(&self) -> &std::path::Path {
        &self.migrations_dir
    }

    fn database_url(&self) -> String {
        self._database.database_url.clone()
    }

    fn migration_schema(&self) -> Option<&str> {
        Some(&self.schema_name)
    }

    fn engine(&self, table: TableId) -> Engine {
        Engine::Postgres(PostgresEngine::new(self.pg_pool.clone(), table))
    }

    fn unique_name(&self, prefix: &str) -> String {
        format!("{}_{}", prefix, self.suffix)
    }

    fn text_type(&self) -> &'static str {
        "VARCHAR(255)"
    }

    fn primary_key_type(&self) -> &'static str {
        "SERIAL PRIMARY KEY"
    }

    fn root_dir(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    async fn table_exists(&self, table_name: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = $1 AND table_name = $2)",
        )
        .bind(&self.schema_name)
        .bind(table_name)
        .fetch_one(&self.pg_pool)
        .await
        .expect("failed to query information_schema.tables")
    }

    async fn migration_table_exists(&self, manager: &MigrationManager) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = $1 AND table_name = $2)",
        )
        .bind(manager.table.schema.as_deref().unwrap_or("public"))
        .bind(&manager.table.name)
        .fetch_one(&self.pg_pool)
        .await
        .expect("failed to query migration table")
    }

    async fn cleanup(self) {
        cleanup_postgres_schema(&self.pg_pool, &self.schema_name).await;
    }
}
