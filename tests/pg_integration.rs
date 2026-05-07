//! PostgreSQL integration tests
//!
//! These tests require a running PostgreSQL instance.
//! Use `docker compose up -d postgres` to start the test database.
//!
//! Connection URL: postgresql://postgres:postgres@localhost:5432/shki_test
//!
//! Run these tests with: `cargo test --test pg_integration -- --ignored`

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use shki::migrate::manager::MigrationManager;
use shki::schema::SqlDialect;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AnyPool, Executor, Pool, Postgres};
use tempfile::TempDir;
use uuid::Uuid;

fn get_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/shki".into())
}

async fn connect_with_retries<T, F, Fut>(label: &str, mut connect: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, sqlx::Error>>,
{
    let max_retries = 5;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        match connect().await {
            Ok(connection) => return connection,
            Err(error) if attempt < max_retries => {
                eprintln!(
                    "{} connection attempt {}/{} failed: {}. Retrying in {:?}...",
                    label, attempt, max_retries, error, retry_delay
                );
                tokio::time::sleep(retry_delay).await;
            }
            Err(error) => {
                panic!(
                    "Failed to connect to PostgreSQL ({}) after {} attempts. Is the database running? Use `docker compose up -d postgres`. Error: {}",
                    label, max_retries, error
                );
            }
        }
    }

    unreachable!()
}

async fn create_pool() -> Pool<Postgres> {
    let url = get_database_url();

    connect_with_retries("Postgres", || {
        PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
    })
    .await
}

async fn create_any_pool() -> AnyPool {
    let url = get_database_url();
    sqlx::any::install_default_drivers();

    connect_with_retries("AnyPool", || {
        sqlx::any::AnyPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
    })
    .await
}

fn unique_schema_name(prefix: &str) -> String {
    let uuid = Uuid::new_v4().to_string().replace('-', "");
    format!("{}_{}", prefix, &uuid[..8])
}

fn schema_suffix(schema_name: &str) -> &str {
    schema_name.rsplit('_').next().unwrap_or(schema_name)
}

fn migration_names(paths: Vec<PathBuf>) -> Vec<String> {
    paths.into_iter()
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string()
        })
        .collect()
}

async fn setup_test_schema(pool: &Pool<Postgres>, schema_name: &str) {
    pool.execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
        .await
        .expect("failed to drop schema");
    pool.execute(format!("CREATE SCHEMA \"{}\"", schema_name).as_str())
        .await
        .expect("failed to create schema");
}

async fn cleanup_test_schema(pool: &Pool<Postgres>, schema_name: &str) {
    pool.execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
        .await
        .expect("failed to cleanup schema");
}

struct PgTestContext {
    pool: AnyPool,
    pg_pool: Pool<Postgres>,
    schema_name: String,
    temp_dir: TempDir,
}

impl PgTestContext {
    async fn new(prefix: &str) -> Self {
        let pg_pool = create_pool().await;
        let pool = create_any_pool().await;
        let schema_name = unique_schema_name(prefix);

        setup_test_schema(&pg_pool, &schema_name).await;

        Self {
            pool,
            pg_pool,
            schema_name,
            temp_dir: TempDir::new().expect("failed to create temp dir"),
        }
    }

    fn manager(&self) -> MigrationManager {
        MigrationManager::new(self.temp_dir.path())
            .with_dialect(SqlDialect::Postgres)
            .with_table_schema(&self.schema_name)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.temp_dir.path().join(name)
    }

    fn write_file(&self, name: &str, contents: impl AsRef<str>) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, contents.as_ref()).expect("failed to write migration file");
        path
    }

    fn write_migrations(&self, files: &[(&str, String)]) {
        for (name, sql) in files {
            self.write_file(name, sql);
        }
    }

    async fn applied_names(&self, manager: &MigrationManager) -> Vec<String> {
        manager
            .get_applied_migrations(&self.pool)
            .await
            .expect("failed to load applied migrations")
            .into_iter()
            .map(|migration| migration.name)
            .collect()
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

    async fn column_exists(&self, table_name: &str, column_name: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 AND column_name = $3)",
        )
        .bind(&self.schema_name)
        .bind(table_name)
        .bind(column_name)
        .fetch_one(&self.pg_pool)
        .await
        .expect("failed to query information_schema.columns")
    }

    async fn unique_constraint_exists(&self, table_name: &str, constraint_name: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.table_constraints WHERE table_schema = $1 AND table_name = $2 AND constraint_name = $3 AND constraint_type = 'UNIQUE')",
        )
        .bind(&self.schema_name)
        .bind(table_name)
        .bind(constraint_name)
        .fetch_one(&self.pg_pool)
        .await
        .expect("failed to query information_schema.table_constraints")
    }

    async fn enum_exists(&self, type_name: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = $1 AND t.typname = $2)",
        )
        .bind(&self.schema_name)
        .bind(type_name)
        .fetch_one(&self.pg_pool)
        .await
        .expect("failed to query pg_type")
    }

    async fn cleanup(self) {
        cleanup_test_schema(&self.pg_pool, &self.schema_name).await;
    }
}

mod migrations {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_apply_simple() {
        let ctx = PgTestContext::new("migrate_simple").await;
        let manager = ctx.manager();
        let table_name = format!("test_table_{}", schema_suffix(&ctx.schema_name));
        let migration_path = ctx.write_file(
            "0001_create_test_table.sql",
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL
                );
                "#,
                schema = ctx.schema_name,
                table = table_name
            ),
        );

        manager
            .apply_migration(&ctx.pool, &migration_path)
            .await
            .expect("failed to apply migration");

        assert!(ctx.table_exists(&table_name).await);
        assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_test_table"]);

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_apply_all_pending() {
        let ctx = PgTestContext::new("migrate_all").await;
        let manager = ctx.manager();
        let suffix = schema_suffix(&ctx.schema_name);
        let users_table = format!("users_{suffix}");
        let posts_table = format!("posts_{suffix}");

        ctx.write_migrations(&[
            (
                "0001_create_users.sql",
                format!(
                    "CREATE TABLE \"{}\".{} (id SERIAL PRIMARY KEY, name VARCHAR(255));",
                    ctx.schema_name, users_table
                ),
            ),
            (
                "0002_create_posts.sql",
                format!(
                    "CREATE TABLE \"{}\".{} (id SERIAL PRIMARY KEY, title VARCHAR(255), user_id INTEGER REFERENCES \"{}\".{}(id));",
                    ctx.schema_name, posts_table, ctx.schema_name, users_table
                ),
            ),
            (
                "0003_add_index.sql",
                format!(
                    "CREATE INDEX idx_posts_{suffix}_user_id ON \"{}\".{}(user_id);",
                    ctx.schema_name, posts_table
                ),
            ),
        ]);

        let applied = manager.apply_all(&ctx.pool).await.expect("failed to apply all");

        assert_eq!(
            applied,
            vec![
                "0001_create_users".to_string(),
                "0002_create_posts".to_string(),
                "0003_add_index".to_string(),
            ]
        );
        assert!(ctx.table_exists(&users_table).await);
        assert!(ctx.table_exists(&posts_table).await);

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_single() {
        let ctx = PgTestContext::new("rollback_single").await;
        let manager = ctx.manager();
        let table_name = format!("rollback_test_{}", schema_suffix(&ctx.schema_name));
        let up_path = ctx.write_file(
            "0001_create_table.sql",
            format!(
                "CREATE TABLE \"{schema}\".\"{table}\" (id SERIAL PRIMARY KEY, name VARCHAR(255));",
                schema = ctx.schema_name,
                table = table_name
            ),
        );
        let down_path = ctx.write_file(
            "0001_create_table.down.sql",
            format!(
                "DROP TABLE \"{schema}\".\"{table}\";",
                schema = ctx.schema_name,
                table = table_name
            ),
        );

        manager
            .apply_migration(&ctx.pool, &up_path)
            .await
            .expect("failed to apply migration");
        assert!(ctx.table_exists(&table_name).await);

        manager
            .rollback_migration(&ctx.pool, &down_path)
            .await
            .expect("failed to rollback migration");

        assert!(!ctx.table_exists(&table_name).await);
        assert!(ctx.applied_names(&manager).await.is_empty());

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_all() {
        let ctx = PgTestContext::new("rollback_all").await;
        let manager = ctx.manager();
        let suffix = schema_suffix(&ctx.schema_name);
        let users_table = format!("users_{suffix}");
        let posts_table = format!("posts_{suffix}");

        ctx.write_migrations(&[
            (
                "0001_create_users.sql",
                format!(
                    "CREATE TABLE \"{schema}\".{table} (id SERIAL PRIMARY KEY);",
                    schema = ctx.schema_name,
                    table = users_table
                ),
            ),
            (
                "0001_create_users.down.sql",
                format!(
                    "DROP TABLE \"{schema}\".{table};",
                    schema = ctx.schema_name,
                    table = users_table
                ),
            ),
            (
                "0002_create_posts.sql",
                format!(
                    "CREATE TABLE \"{schema}\".{table} (id SERIAL PRIMARY KEY);",
                    schema = ctx.schema_name,
                    table = posts_table
                ),
            ),
            (
                "0002_create_posts.down.sql",
                format!(
                    "DROP TABLE \"{schema}\".{table};",
                    schema = ctx.schema_name,
                    table = posts_table
                ),
            ),
        ]);

        manager.apply_all(&ctx.pool).await.expect("failed to apply all");
        assert!(ctx.table_exists(&users_table).await);
        assert!(ctx.table_exists(&posts_table).await);

        let rolled_back = manager
            .rollback_all(&ctx.pool)
            .await
            .expect("failed to rollback all");

        assert_eq!(
            rolled_back,
            vec![
                "0002_create_posts".to_string(),
                "0001_create_users".to_string(),
            ]
        );
        assert!(!ctx.table_exists(&users_table).await);
        assert!(!ctx.table_exists(&posts_table).await);

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_count() {
        let ctx = PgTestContext::new("rollback_count").await;
        let manager = ctx.manager();
        let suffix = schema_suffix(&ctx.schema_name);

        for i in 1..=3 {
            ctx.write_file(
                &format!("000{}_create_tbl{}.sql", i, i),
                format!(
                    "CREATE TABLE \"{schema}\".tbl{i}_{suffix} (id SERIAL PRIMARY KEY);",
                    schema = ctx.schema_name,
                ),
            );
            ctx.write_file(
                &format!("000{}_create_tbl{}.down.sql", i, i),
                format!(
                    "DROP TABLE \"{schema}\".tbl{i}_{suffix};",
                    schema = ctx.schema_name,
                ),
            );
        }

        manager.apply_all(&ctx.pool).await.expect("failed to apply all");

        let rolled_back = manager
            .rollback_count(&ctx.pool, 2)
            .await
            .expect("failed to rollback count");

        assert_eq!(
            rolled_back,
            vec!["0003_create_tbl3".to_string(), "0002_create_tbl2".to_string()]
        );
        assert!(ctx.table_exists(&format!("tbl1_{suffix}")).await);
        assert!(!ctx.table_exists(&format!("tbl2_{suffix}")).await);
        assert!(!ctx.table_exists(&format!("tbl3_{suffix}")).await);
        assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_tbl1"]);

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_pending_detection() {
        let ctx = PgTestContext::new("pending").await;
        let manager = ctx.manager();
        let suffix = schema_suffix(&ctx.schema_name);

        for i in 1..=3 {
            ctx.write_file(
                &format!("000{}_migration_{}.sql", i, i),
                format!(
                    "CREATE TABLE \"{schema}\".pending_t{i}_{suffix} (id SERIAL PRIMARY KEY);",
                    schema = ctx.schema_name,
                ),
            );
        }

        assert_eq!(
            migration_names(
                manager
                    .get_pending_migrations(&ctx.pool)
                    .await
                    .expect("failed to get initial pending migrations")
            ),
            vec![
                "0001_migration_1".to_string(),
                "0002_migration_2".to_string(),
                "0003_migration_3".to_string(),
            ]
        );

        manager
            .apply_migration(&ctx.pool, &ctx.path("0001_migration_1.sql"))
            .await
            .expect("failed to apply first migration");

        assert_eq!(
            migration_names(
                manager
                    .get_pending_migrations(&ctx.pool)
                    .await
                    .expect("failed to get pending migrations after one apply")
            ),
            vec![
                "0002_migration_2".to_string(),
                "0003_migration_3".to_string(),
            ]
        );

        manager.apply_all(&ctx.pool).await.expect("failed to apply remaining");

        assert!(
            manager
                .get_pending_migrations(&ctx.pool)
                .await
                .expect("failed to get final pending migrations")
                .is_empty()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_transaction_rollback_on_error() {
        let ctx = PgTestContext::new("tx_rollback").await;
        let manager = ctx.manager();
        let table_name = format!("good_table_{}", schema_suffix(&ctx.schema_name));
        let migration_path = ctx.write_file(
            "0001_bad_migration.sql",
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (id SERIAL PRIMARY KEY);
                CREATE TABLE "{schema}"."{table}" (id SERIAL PRIMARY KEY);
                "#,
                schema = ctx.schema_name,
                table = table_name
            ),
        );

        let result = manager.apply_migration(&ctx.pool, &migration_path).await;

        assert!(result.is_err());
        assert!(!ctx.table_exists(&table_name).await);
        assert!(ctx.applied_names(&manager).await.is_empty());

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_alter_table() {
        let ctx = PgTestContext::new("alter").await;
        let manager = ctx.manager();
        let table_name = format!("users_{}", schema_suffix(&ctx.schema_name));
        let constraint_name = format!("{}_email_unique", table_name);

        ctx.write_migrations(&[
            (
                "0001_create_users.sql",
                format!(
                    "CREATE TABLE \"{schema}\".\"{table}\" (id SERIAL PRIMARY KEY, name VARCHAR(255));",
                    schema = ctx.schema_name,
                    table = table_name
                ),
            ),
            (
                "0002_add_email.sql",
                format!(
                    "ALTER TABLE \"{schema}\".\"{table}\" ADD COLUMN email VARCHAR(255);",
                    schema = ctx.schema_name,
                    table = table_name
                ),
            ),
            (
                "0002_add_email.down.sql",
                format!(
                    "ALTER TABLE \"{schema}\".\"{table}\" DROP COLUMN email;",
                    schema = ctx.schema_name,
                    table = table_name
                ),
            ),
            (
                "0003_add_unique.sql",
                format!(
                    "ALTER TABLE \"{schema}\".\"{table}\" ADD CONSTRAINT {constraint} UNIQUE (email);",
                    schema = ctx.schema_name,
                    table = table_name,
                    constraint = constraint_name
                ),
            ),
            (
                "0003_add_unique.down.sql",
                format!(
                    "ALTER TABLE \"{schema}\".\"{table}\" DROP CONSTRAINT {constraint};",
                    schema = ctx.schema_name,
                    table = table_name,
                    constraint = constraint_name
                ),
            ),
        ]);

        manager.apply_all(&ctx.pool).await.expect("failed to apply all");

        assert!(ctx.column_exists(&table_name, "email").await);
        assert!(
            ctx.unique_constraint_exists(&table_name, &constraint_name)
                .await
        );

        manager
            .rollback_count(&ctx.pool, 1)
            .await
            .expect("failed to rollback unique constraint");

        assert!(ctx.column_exists(&table_name, "email").await);
        assert!(
            !ctx.unique_constraint_exists(&table_name, &constraint_name)
                .await
        );

        manager
            .rollback_count(&ctx.pool, 1)
            .await
            .expect("failed to rollback email column");

        assert!(!ctx.column_exists(&table_name, "email").await);

        ctx.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_with_enum_type() {
        let ctx = PgTestContext::new("enum_migration").await;
        let manager = ctx.manager();
        let suffix = schema_suffix(&ctx.schema_name);
        let enum_name = format!("order_status_{suffix}");
        let table_name = format!("orders_{suffix}");

        ctx.write_migrations(&[
            (
                "0001_create_enum.sql",
                format!(
                    r#"
                    CREATE TYPE "{schema}".{enum_name} AS ENUM ('pending', 'processing', 'shipped', 'delivered');
                    CREATE TABLE "{schema}".{table_name} (
                        id SERIAL PRIMARY KEY,
                        status "{schema}".{enum_name} NOT NULL DEFAULT 'pending'
                    );
                    "#,
                    schema = ctx.schema_name,
                    enum_name = enum_name,
                    table_name = table_name
                ),
            ),
            (
                "0001_create_enum.down.sql",
                format!(
                    r#"
                    DROP TABLE "{schema}".{table_name};
                    DROP TYPE "{schema}".{enum_name};
                    "#,
                    schema = ctx.schema_name,
                    enum_name = enum_name,
                    table_name = table_name
                ),
            ),
        ]);

        manager.apply_all(&ctx.pool).await.expect("failed to apply enum migration");

        assert!(ctx.enum_exists(&enum_name).await);
        assert!(ctx.table_exists(&table_name).await);

        manager
            .rollback_all(&ctx.pool)
            .await
            .expect("failed to rollback enum migration");

        assert!(!ctx.enum_exists(&enum_name).await);
        assert!(!ctx.table_exists(&table_name).await);

        ctx.cleanup().await;
    }
}
