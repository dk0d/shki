use sqlx::Pool;
use sqlx::mysql::MySqlPoolOptions;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::ReuseDirective;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::mysql::Mysql as MysqlContainer;
use tokio::sync::{OnceCell, OwnedSemaphorePermit, Semaphore};
use tokio::time::sleep;

use super::TestBackend;
use shki::engines::Engine;
use shki::engines::mysql::Mysql;
use shki::migrate::manager::MigrationManager;
use shki::models::iden::Iden;
use shki::schema::SqlDialect;

use crate::common::{connect_with_retries, unique_suffix};

struct SharedMysqlServer {
    _container: ContainerAsync<MysqlContainer>,
    admin_url: String,
}

static MYSQL_SERVER: OnceCell<SharedMysqlServer> = OnceCell::const_new();
static MYSQL_TEST_LIMIT: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(1)));

async fn shared_mysql_server() -> &'static SharedMysqlServer {
    MYSQL_SERVER
        .get_or_init(|| async {
            let container = MysqlContainer::default()
                .with_name("mysql")
                .with_reuse(ReuseDirective::Always)
                .with_tag("8.0.34")
                .with_startup_timeout(Duration::from_secs(120))
                .start()
                .await
                .expect("failed to start shared mysql test container");

            let host = container
                .get_host()
                .await
                .expect("failed to get mysql test container host");
            let mut last_error = None;
            let mut port = None;
            for _ in 0..40 {
                match container.get_host_port_ipv4(3306).await {
                    Ok(bound_port) => {
                        port = Some(bound_port);
                        break;
                    }
                    Err(error) => {
                        last_error = Some(error);
                        sleep(Duration::from_millis(250)).await;
                    }
                }
            }
            let port = port.unwrap_or_else(|| {
                panic!(
                    "failed to get mysql test container port after waiting: {:?}",
                    last_error
                )
            });

            SharedMysqlServer {
                admin_url: format!("mysql://root@{host}:{port}/mysql"),
                _container: container,
            }
        })
        .await
}

pub struct MysqlTestContext {
    pub database_name: String,
    pub database_url: String,
    pub mysql_pool: Pool<sqlx::MySql>,
    pub temp_dir: TempDir,
    pub migrations_dir: PathBuf,
    pub suffix: String,
    _permit: OwnedSemaphorePermit,
}

impl MysqlTestContext {
    pub async fn new(name: &str) -> Self {
        let permit = MYSQL_TEST_LIMIT
            .clone()
            .acquire_owned()
            .await
            .expect("failed to acquire mysql test permit");
        let server = shared_mysql_server().await;
        let database_name = format!("shki_{}_{}", name, unique_suffix());

        let admin_pool = connect_with_retries("MySQL admin", || {
            MySqlPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&server.admin_url)
        })
        .await;

        sqlx::query(&format!("CREATE DATABASE `{database_name}`"))
            .execute(&admin_pool)
            .await
            .expect("failed to create mysql test database");

        let database_url = server
            .admin_url
            .replace("/mysql", &format!("/{database_name}"));
        let mysql_pool = connect_with_retries("MySQL", || {
            MySqlPoolOptions::new()
                .max_connections(2)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
        })
        .await;

        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir_all(&migrations_dir).expect("failed to create migrations dir");

        Self {
            database_name,
            database_url,
            mysql_pool,
            temp_dir,
            migrations_dir,
            suffix: unique_suffix(),
            _permit: permit,
        }
    }
}

impl TestBackend for MysqlTestContext {
    async fn setup(name: &str) -> Self {
        Self::new(name).await
    }

    fn dialect(&self) -> SqlDialect {
        SqlDialect::Mysql
    }

    fn migrations_dir(&self) -> &std::path::Path {
        &self.migrations_dir
    }

    fn database_url(&self) -> String {
        self.database_url.clone()
    }

    fn migration_schema(&self) -> Option<&str> {
        None
    }

    fn engine(&self, table: Iden) -> Engine {
        Engine::Mysql(Mysql::new(self.mysql_pool.clone(), table))
    }

    fn unique_name(&self, prefix: &str) -> String {
        format!("{}_{}", prefix, self.suffix)
    }

    fn text_type(&self) -> &'static str {
        "VARCHAR(255)"
    }

    fn primary_key_type(&self) -> &'static str {
        "INT AUTO_INCREMENT PRIMARY KEY"
    }

    fn root_dir(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    async fn table_exists(&self, table_name: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?)",
        )
        .bind(table_name)
        .fetch_one(&self.mysql_pool)
        .await
        .expect("failed to query information_schema.tables")
    }

    async fn migration_table_exists(&self, manager: &MigrationManager) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?)",
        )
        .bind(&manager.table.name)
        .fetch_one(&self.mysql_pool)
        .await
        .expect("failed to query migration table")
    }

    async fn cleanup(self) {
        self.mysql_pool.close().await;
        let server = shared_mysql_server().await;
        let admin_pool = connect_with_retries("MySQL admin", || {
            MySqlPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&server.admin_url)
        })
        .await;

        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", self.database_name))
            .execute(&admin_pool)
            .await
            .expect("failed to drop mysql test database");
    }
}
