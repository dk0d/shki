use sqlx::any::AnyPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AnyPool, Executor, Pool, Postgres};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PostgresContainer;

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
    pub pool: AnyPool,
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

        let url = database.database_url.clone();
        sqlx::any::install_default_drivers();
        let pool = connect_with_retries("AnyPool", || {
            AnyPoolOptions::new()
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
            pool,
            pg_pool,
            schema_name,
            temp_dir,
            migrations_dir,
            suffix: unique_suffix(),
        }
    }
}
