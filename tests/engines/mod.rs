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
use sqlx::{AssertSqlSafe, Executor, Pool, Postgres};
use std::path::{Path, PathBuf};

/// Shared server containers live in statics, so testcontainers' `Drop`-based
/// cleanup never runs for them. Register their IDs here; an `atexit` hook
/// removes them when the test process exits.
pub fn remove_container_on_exit(id: &str) {
    use std::sync::{LazyLock, Mutex, Once};

    static IDS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(Mutex::default);
    static HOOK: Once = Once::new();

    unsafe extern "C" {
        fn atexit(cb: extern "C" fn()) -> i32;
    }
    // ponytail: shells out to the docker CLI; switch to the bollard client if
    // a docker-CLI-less environment ever runs these tests.
    extern "C" fn remove_all() {
        for id in IDS.lock().unwrap().drain(..) {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", "-v", &id])
                .output();
        }
    }

    HOOK.call_once(|| unsafe {
        atexit(remove_all);
    });
    IDS.lock().unwrap().push(id.to_string());
}

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
schema = "schema"
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
            common: self.common_args(),
            command: Commands::Migrate {
                migrations: Default::default(),
                mode: None,
                dry_run: false,
            },
        }
    }

    fn down_cli(&self, config_path: PathBuf, count: Option<usize>, dry_run: bool) -> Cli {
        Cli {
            config: config_path,
            common: self.common_args(),
            command: Commands::Down {
                migrations: Default::default(),
                count,
                dry_run,
            },
        }
    }

    fn common_args(&self) -> CommonArgs {
        CommonArgs {
            dialect: Some(self.dialect()),
            database_url: Some(self.database_url()),
            ..CommonArgs::default()
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
    let query = format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name);
    pool.execute(AssertSqlSafe(query))
        .await
        .expect("failed to cleanup schema");
}
