pub mod mysql;
pub mod pg;
pub mod sqlite;

pub use mysql::*;
pub use pg::*;
pub use sqlite::*;

use shki::engines::Engine;
use shki::migrate::manager::MigrationManager;
use shki::models::iden::Iden;
use shki::schema::SqlDialect;
use shki::{Cli, Commands, CommonArgs};
use sqlx::{Executor, Pool, Postgres};
use std::path::{Path, PathBuf};

pub trait TestBackend: Sized {
    async fn setup(name: &str) -> Self;

    fn dialect(&self) -> SqlDialect;
    fn migrations_dir(&self) -> &Path;
    fn database_url(&self) -> String;
    fn migration_schema(&self) -> Option<&str>;
    fn engine(&self, table: Iden) -> Engine;
    fn unique_name(&self, prefix: &str) -> String;
    fn text_type(&self) -> &'static str;
    fn primary_key_type(&self) -> &'static str;
    fn root_dir(&self) -> &Path;

    async fn table_exists(&self, table_name: &str) -> bool;
    async fn migration_table_exists(&self, manager: &MigrationManager) -> bool;
    async fn cleanup(self);

    fn manager(&self) -> MigrationManager {
        self.manager_with_table("__shki_migrations")
    }

    fn manager_with_table(&self, table_name: &str) -> MigrationManager {
        let table: Iden = (
            table_name.to_string(),
            self.migration_schema().map(str::to_string),
        )
            .into();
        MigrationManager::new(self.migrations_dir(), self.engine(table))
    }

    fn create_table_sql(&self, table: &str, extra_columns: &[String]) -> String {
        let mut columns = vec![format!("id {}", self.primary_key_type())];
        columns.extend(extra_columns.iter().cloned());
        let columns = columns.join(", ");

        match self.migration_schema() {
            Some(schema) => format!("CREATE TABLE \"{schema}\".\"{table}\" ({columns});"),
            None => format!("CREATE TABLE {table} ({columns});"),
        }
    }

    fn drop_table_sql(&self, table: &str) -> String {
        match self.migration_schema() {
            Some(schema) => format!("DROP TABLE \"{schema}\".\"{table}\";"),
            None => format!("DROP TABLE {table};"),
        }
    }

    fn write_migration(&self, name: &str, sql: &str) -> PathBuf {
        let path = self.migrations_dir().join(name);
        std::fs::write(&path, sql).expect("failed to write migration");
        path
    }

    fn write_migrations(&self, files: &[(&str, String)]) {
        for (name, sql) in files {
            self.write_migration(name, sql);
        }
    }

    fn write_config(&self) -> PathBuf {
        let config_path = self.root_dir().join("shki.toml");
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
                self.root_dir().display(),
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

    fn migrate_cli(&self, config_path: PathBuf) -> Cli {
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

    fn down_cli(&self, config_path: PathBuf, count: Option<usize>, dry_run: bool) -> Cli {
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

    async fn applied_names(&self, manager: &MigrationManager) -> Vec<String> {
        manager
            .get_applied_migrations()
            .await
            .expect("failed to load applied migrations")
            .into_iter()
            .map(|migration| migration.name)
            .collect()
    }
}

pub async fn cleanup_postgres_schema(pool: &Pool<Postgres>, schema_name: &str) {
    pool.execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
        .await
        .expect("failed to cleanup schema");
}
