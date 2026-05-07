pub mod pg;
pub mod sqlite;
pub use pg::*;

use shki::migrate::manager::MigrationManager;
use shki::schema::SqlDialect;
use shki::{Cli, Commands, CommonArgs};
pub use sqlite::*;
use sqlx::{AnyPool, Executor};
use std::path::{Path, PathBuf};

pub enum BackendContext {
    Sqlite(Box<SqliteTestContext>),
    Postgres(Box<PgTestContext>),
}

impl BackendContext {
    pub async fn sqlite(name: &str) -> Self {
        Self::Sqlite(Box::new(SqliteTestContext::new(name)))
    }

    pub async fn postgres(name: &str) -> Self {
        Self::Postgres(Box::new(PgTestContext::new(name).await))
    }

    pub fn dialect(&self) -> SqlDialect {
        match self {
            Self::Sqlite(_) => SqlDialect::Sqlite,
            Self::Postgres(_) => SqlDialect::Postgres,
        }
    }

    pub fn migrations_dir(&self) -> &Path {
        match self {
            Self::Sqlite(ctx) => &ctx.migrations_dir,
            Self::Postgres(ctx) => &ctx.migrations_dir,
        }
    }

    pub fn database_url(&self) -> String {
        match self {
            Self::Sqlite(ctx) => ctx.url(),
            Self::Postgres(ctx) => ctx._database.database_url.clone(),
        }
    }

    pub fn migration_schema(&self) -> Option<&str> {
        match self {
            Self::Sqlite(_) => None,
            Self::Postgres(ctx) => Some(&ctx.schema_name),
        }
    }

    pub fn manager(&self) -> MigrationManager {
        let manager = MigrationManager::new(self.migrations_dir()).with_dialect(self.dialect());
        match self.migration_schema() {
            Some(schema) => manager.with_table_schema(schema),
            None => manager,
        }
    }

    pub async fn pool(&self) -> AnyPool {
        match self {
            Self::Sqlite(ctx) => ctx.pool().await,
            Self::Postgres(ctx) => ctx.pool.clone(),
        }
    }

    pub fn unique_name(&self, prefix: &str) -> String {
        match self {
            Self::Sqlite(ctx) => format!("{}_{}", prefix, ctx.suffix),
            Self::Postgres(ctx) => format!("{}_{}", prefix, ctx.suffix),
        }
    }

    pub fn text_type(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "TEXT",
            Self::Postgres(_) => "VARCHAR(255)",
        }
    }

    fn primary_key_type(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "INTEGER PRIMARY KEY",
            Self::Postgres(_) => "SERIAL PRIMARY KEY",
        }
    }

    pub fn create_table_sql(&self, table: &str, extra_columns: &[String]) -> String {
        let mut columns = vec![format!("id {}", self.primary_key_type())];
        columns.extend(extra_columns.iter().cloned());
        let columns = columns.join(", ");

        match self {
            Self::Sqlite(_) => format!("CREATE TABLE {table} ({columns});"),
            Self::Postgres(ctx) => {
                format!(
                    "CREATE TABLE \"{}\".\"{table}\" ({columns});",
                    ctx.schema_name
                )
            }
        }
    }

    pub fn drop_table_sql(&self, table: &str) -> String {
        match self {
            Self::Sqlite(_) => format!("DROP TABLE {table};"),
            Self::Postgres(ctx) => format!("DROP TABLE \"{}\".\"{table}\";", ctx.schema_name),
        }
    }

    pub fn write_migration(&self, name: &str, sql: &str) -> PathBuf {
        let path = self.migrations_dir().join(name);
        std::fs::write(&path, sql).expect("failed to write migration");
        path
    }

    pub fn write_migrations(&self, files: &[(&str, String)]) {
        for (name, sql) in files {
            self.write_migration(name, sql);
        }
    }

    pub fn write_config(&self) -> PathBuf {
        let root = match self {
            Self::Sqlite(ctx) => ctx.temp_dir.path(),
            Self::Postgres(ctx) => ctx.temp_dir.path(),
        };
        let config_path = root.join("shki.toml");

        let migrations_section = match self.migration_schema() {
            Some(schema) => format!("\n[migrations]\nschema = \"{schema}\"\n"),
            None => String::new(),
        };

        std::fs::write(
            &config_path,
            format!(
                r#"
root = "{}"
dialect = "{}"
schema = "init.lua"
out = "migrations"
database_url = "{}"
{}"#,
                root.display(),
                match self.dialect() {
                    SqlDialect::Postgres => "postgres",
                    SqlDialect::Sqlite => "sqlite",
                    SqlDialect::Mysql => "mysql",
                },
                self.database_url(),
                migrations_section,
            ),
        )
        .expect("failed to write config");

        config_path
    }

    pub fn migrate_cli(&self, config_path: PathBuf) -> Cli {
        Cli {
            config: config_path,
            common: CommonArgs {
                dialect: Some(self.dialect()),
                database_url: Some(self.database_url()),
                ..CommonArgs::default()
            },
            command: Commands::Migrate,
        }
    }

    pub fn down_cli(&self, config_path: PathBuf, count: Option<usize>, dry_run: bool) -> Cli {
        Cli {
            config: config_path,
            common: CommonArgs {
                dialect: Some(self.dialect()),
                database_url: Some(self.database_url()),
                ..CommonArgs::default()
            },
            command: Commands::Down { count, dry_run },
        }
    }

    pub async fn table_exists(&self, table_name: &str) -> bool {
        match self {
            Self::Sqlite(ctx) => sqlx::query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table_name)
            .fetch_optional(&ctx.pool().await)
            .await
            .expect("failed to query sqlite_master")
            .is_some(),
            Self::Postgres(ctx) => {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = $1 AND table_name = $2)",
                )
                .bind(&ctx.schema_name)
                .bind(table_name)
                .fetch_one(&ctx.pg_pool)
                .await
                .expect("failed to query information_schema.tables")
            }
        }
    }

    pub async fn migration_table_exists(&self, manager: &MigrationManager) -> bool {
        match self {
            Self::Sqlite(ctx) => sqlx::query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(&manager.table_name)
            .fetch_optional(&ctx.pool().await)
            .await
            .expect("failed to query sqlite_master")
            .is_some(),
            Self::Postgres(ctx) => {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = $1 AND table_name = $2)",
                )
                .bind(manager.table_schema.as_deref().unwrap_or("public"))
                .bind(&manager.table_name)
                .fetch_one(&ctx.pg_pool)
                .await
                .expect("failed to query migration table")
            }
        }
    }

    pub async fn applied_names(&self, manager: &MigrationManager) -> Vec<String> {
        manager
            .get_applied_migrations(&self.pool().await)
            .await
            .expect("failed to load applied migrations")
            .into_iter()
            .map(|migration| migration.name)
            .collect()
    }

    pub async fn cleanup(self) {
        if let Self::Postgres(ctx) = self {
            ctx.pg_pool
                .execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", ctx.schema_name).as_str())
                .await
                .expect("failed to cleanup schema");
        }
    }
}
