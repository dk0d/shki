use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, Pool, Postgres};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::ReuseDirective;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PostgresContainer;
use tokio::sync::OnceCell;

use super::{TestBackend, cleanup_postgres_schema};
use shki::engines::Engine;
use shki::engines::pg::Postgres as PostgresEngine;
use shki::migrate::manager::MigrationManager;
use shki::models::iden::Iden;
use shki::schema::SqlDialect;

use crate::common::{connect_with_retries, unique_suffix};

struct SharedPostgresServer {
    _container: ContainerAsync<PostgresContainer>,
    admin_url: String,
}

static POSTGRES_SERVER: OnceCell<SharedPostgresServer> = OnceCell::const_new();

async fn shared_postgres_server() -> &'static SharedPostgresServer {
    POSTGRES_SERVER
        .get_or_init(|| async {
            let image = PostgresContainer::default()
                .with_db_name("postgres")
                .with_user("postgres")
                .with_password("postgres")
                .with_tag("16-alpine")
                .with_container_name("shki-postgres-tests-v2")
                .with_reuse(ReuseDirective::Always);

            let container = image
                .start()
                .await
                .expect("failed to start shared postgres test container");

            let host = container
                .get_host()
                .await
                .expect("failed to get postgres test container host");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("failed to get postgres test container port");

            SharedPostgresServer {
                admin_url: format!("postgresql://postgres:postgres@{host}:{port}/postgres"),
                _container: container,
            }
        })
        .await
}

pub struct TestDatabase {
    pub database_url: String,
    database_name: String,
}

impl TestDatabase {
    pub async fn start() -> Self {
        let server = shared_postgres_server().await;
        let database_name = format!("shki_shadow_{}", unique_suffix());
        let admin_pool = connect_with_retries("Postgres admin", || {
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&server.admin_url)
        })
        .await;

        admin_pool
            .execute(format!("CREATE DATABASE \"{}\"", database_name).as_str())
            .await
            .expect("failed to create postgres shadow database");

        Self {
            database_url: server
                .admin_url
                .strip_suffix("/postgres")
                .expect("postgres admin URL should include database path")
                .to_string()
                + &format!("/{database_name}"),
            database_name,
        }
    }

    pub async fn cleanup(self) {
        let server = shared_postgres_server().await;
        let admin_pool = connect_with_retries("Postgres admin", || {
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&server.admin_url)
        })
        .await;

        admin_pool
            .execute(
                format!(
                    "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
                    self.database_name
                )
                .as_str(),
            )
            .await
            .expect("failed to drop postgres shadow database");
    }
}

pub struct PgTestContext {
    pub database_url: String,
    pub pg_pool: Pool<Postgres>,
    pub schema_name: String,
    pub temp_dir: TempDir,
    pub migrations_dir: PathBuf,
    pub suffix: String,
}

impl PgTestContext {
    pub async fn new(name: &str) -> Self {
        let server = shared_postgres_server().await;
        let database_url = server.admin_url.clone();
        let pg_pool = connect_with_retries("Postgres", || {
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
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
            database_url,
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
        self.database_url.clone()
    }

    fn migration_schema(&self) -> Option<&str> {
        Some(&self.schema_name)
    }

    fn engine(&self, table: Iden) -> Engine {
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
