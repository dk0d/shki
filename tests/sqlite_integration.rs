use std::path::{Path, PathBuf};

use shki::migrate::manager::MigrationManager;
use shki::schema::SqlDialect;
use shki::{Cli, Commands, CommonArgs, run};
use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;
use tempfile::TempDir;

fn sqlite_file_url(db_path: &Path) -> String {
    format!("sqlite://{}", db_path.display())
}

async fn create_sqlite_pool(db_path: &Path) -> AnyPool {
    sqlx::any::install_default_drivers();

    AnyPoolOptions::new()
        .max_connections(1)
        .connect(&sqlite_file_url(db_path))
        .await
        .expect("failed to connect to sqlite test db")
}

async fn table_exists(pool: &AnyPool, table_name: &str) -> bool {
    sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table_name)
        .fetch_optional(pool)
        .await
        .expect("failed to query sqlite_master")
        .is_some()
}

struct SqliteTestContext {
    temp_dir: TempDir,
    db_path: PathBuf,
    migrations_dir: PathBuf,
}

impl SqliteTestContext {
    fn new(db_name: &str) -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join(db_name);
        let migrations_dir = temp_dir.path().join("migrations");

        std::fs::File::create(&db_path).expect("failed to create sqlite db file");
        std::fs::create_dir_all(&migrations_dir).expect("failed to create migrations dir");

        Self {
            temp_dir,
            db_path,
            migrations_dir,
        }
    }

    fn url(&self) -> String {
        sqlite_file_url(&self.db_path)
    }

    async fn pool(&self) -> AnyPool {
        create_sqlite_pool(&self.db_path).await
    }

    fn manager(&self) -> MigrationManager {
        MigrationManager::new(&self.migrations_dir).with_dialect(SqlDialect::Sqlite)
    }

    fn write_migration(&self, name: &str, sql: &str) -> PathBuf {
        let path = self.migrations_dir.join(name);
        std::fs::write(&path, sql).expect("failed to write migration");
        path
    }

    fn write_config(&self) -> PathBuf {
        let config_path = self.temp_dir.path().join("shki.toml");

        std::fs::write(
            &config_path,
            format!(
                r#"
root = "{}"
dialect = "sqlite"
schema = "init.lua"
out = "migrations"
database_url = "{}"
"#,
                self.temp_dir.path().display(),
                self.url()
            ),
        )
        .expect("failed to write config");

        config_path
    }

    fn migrate_cli(&self, config_path: PathBuf) -> Cli {
        Cli {
            config: config_path,
            common: CommonArgs {
                dialect: Some(SqlDialect::Sqlite),
                database_url: Some(self.url()),
                ..CommonArgs::default()
            },
            command: Commands::Migrate,
        }
    }
}

#[tokio::test]
async fn test_sqlite_file_migration_apply_persists_between_pools() {
    let ctx = SqliteTestContext::new("apply.db");
    let manager = ctx.manager();
    let migration_path = ctx.write_migration(
        "0001_create_users.sql",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
    );
    let pool = ctx.pool().await;

    manager
        .apply_migration(&pool, &migration_path)
        .await
        .expect("failed to apply sqlite migration");

    drop(pool);

    let reopened_pool = ctx.pool().await;
    assert!(table_exists(&reopened_pool, "users").await);

    let applied = manager
        .get_applied_migrations(&reopened_pool)
        .await
        .expect("failed to load applied migrations");

    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].name, "0001_create_users");
    assert!(applied[0].checksum.is_some());
}

#[tokio::test]
async fn test_sqlite_file_migration_rollback_removes_table_and_record() {
    let ctx = SqliteTestContext::new("rollback.db");
    let manager = ctx.manager();
    let up_path = ctx.write_migration(
        "0001_create_widgets.sql",
        "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
    );
    let down_path = ctx.write_migration("0001_create_widgets.down.sql", "DROP TABLE widgets;");
    let pool = ctx.pool().await;

    manager
        .apply_migration(&pool, &up_path)
        .await
        .expect("failed to apply sqlite migration");
    assert!(table_exists(&pool, "widgets").await);

    manager
        .rollback_migration(&pool, &down_path)
        .await
        .expect("failed to rollback sqlite migration");

    assert!(!table_exists(&pool, "widgets").await);
    assert!(
        manager
            .get_applied_migrations(&pool)
            .await
            .expect("failed to load applied migrations after rollback")
            .is_empty()
    );
}

#[tokio::test]
async fn test_sqlite_file_cli_migrate_applies_pending_migrations() {
    let ctx = SqliteTestContext::new("cli.db");
    let manager = ctx.manager();

    ctx.write_migration(
        "0001_create_posts.sql",
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT NOT NULL);",
    );

    run(ctx.migrate_cli(ctx.write_config()))
        .await
        .expect("cli migrate failed for sqlite file");

    let pool = ctx.pool().await;
    assert!(table_exists(&pool, "posts").await);

    let applied = manager
        .get_applied_migrations(&pool)
        .await
        .expect("failed to load applied migrations after cli run");

    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].name, "0001_create_posts");
}
