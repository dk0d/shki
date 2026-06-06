//! Configuration for Shki
//!
//! This module provides configuration structures for the CLI and library.

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    CommonArgs, ShkiError, codegen::CodegenConfig, models::iden::Iden, schema::SqlDialect,
    utils::resolve_path,
};
use clap::ValueEnum;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Default, Deserialize)]
struct ExplicitConfigProbe {
    dialect: Option<SqlDialect>,
}

/// Schema definition language
#[derive(Debug, Clone, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaMode {
    /// Just write SQL migration files without any schema definition or diffing
    /// (disables schema path config)
    #[default]
    Sql,

    /// Define schemas using Lua scripts
    #[serde(alias = "ts")]
    #[value(alias = "ts")]
    Typescript,
}

impl std::fmt::Display for SchemaMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaMode::Sql => write!(f, "sql"),
            SchemaMode::Typescript => write!(f, "typescript"),
        }
    }
}

/// Main configuration for Shki
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Project root used to resolve relative paths
    #[serde(default = "default_root")]
    pub root: PathBuf,

    /// Database dialect
    pub dialect: SqlDialect,

    /// Path to schema dir/file (unused when mode is `sql)
    #[serde(default)]
    pub schema: PathBuf,

    /// Output directory for migrations
    #[serde(default = "default_out")]
    pub out: PathBuf,

    /// Database connection URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    /// Disposable Shadow Database URL used to compile Declarative Schemas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_database_url: Option<String>,

    /// PostgreSQL major version for embedded Shadow Database compilation, defaults to 18
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pg_version: Option<u8>,

    /// Whether to add breakpoints between SQL statements
    #[serde(default = "default_true")]
    pub breakpoints: bool,

    /// Verbose output
    #[serde(default)]
    pub verbose: bool,

    /// Migration settings
    #[serde(default)]
    pub migrations: MigrationConfig,

    /// Database connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default)]
    pub mode: SchemaMode,

    #[serde(default)]
    pub codegen: CodegenConfig,

    #[serde(default = "default_false")]
    pub no_color: bool,
}

fn default_false() -> bool {
    false
}

fn default_timeout() -> u64 {
    2
}

fn default_root() -> PathBuf {
    PathBuf::from("./")
}

fn default_out() -> PathBuf {
    PathBuf::from("./migrations")
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationTableId {
    /// Name of the migrations table
    #[serde(default = "default_migrations_table")]
    pub name: String,

    /// Schema name for the migrations table (PostgreSQL)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_migrations_schema"
    )]
    pub schema: Option<String>,
}

impl Default for MigrationTableId {
    fn default() -> Self {
        Self {
            name: default_migrations_table(),
            schema: default_migrations_schema(),
        }
    }
}

impl From<MigrationTableId> for Iden {
    fn from(config: MigrationTableId) -> Self {
        Self {
            schema: config.schema,
            name: config.name,
        }
    }
}

/// Migration-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Name of the migrations table
    #[serde(default = "default_migrations_table")]
    pub table: String,

    /// Schema for the migrations table (PostgreSQL)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_migrations_schema"
    )]
    pub schema: Option<String>,

    /// Migration file name prefix style
    #[serde(default)]
    pub prefix: MigrationPrefix,

    /// Whether to generate down migrations alongside up migrations
    #[serde(default)]
    pub generate_down: bool,
}

fn default_migrations_table() -> String {
    "__shki_migrations".to_string()
}

fn default_migrations_schema() -> Option<String> {
    "shki".to_string().into()
}

impl MigrationConfig {
    pub fn entity(&self) -> Iden {
        (self.table.clone(), self.schema.clone()).into()
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            prefix: MigrationPrefix::Index,
            generate_down: false,
            table: default_migrations_table(),
            schema: default_migrations_schema(),
        }
    }
}

/// Migration file name prefix style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum MigrationPrefix {
    /// Sequential index (0000, 0001, 0002, ...)
    #[default]
    Index,

    /// Timestamp (20240101120000)
    Timestamp,

    /// Unix timestamp
    Unix,
}

/// Identifier casing style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IdentifierCasing {
    /// Preserve original casing from database
    #[default]
    Preserve,

    /// Convert to camelCase
    Camel,

    /// Convert to snake_case
    Snake,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: default_root(),
            dialect: SqlDialect::default(),
            mode: SchemaMode::default(),
            schema: default_schema_path(),
            out: default_out(),
            database_url: None,
            shadow_database_url: None,
            pg_version: None,
            breakpoints: true,
            verbose: false,
            no_color: false,
            codegen: CodegenConfig::default(),
            migrations: MigrationConfig::default(),
            // introspect: IntrospectConfig::default(),
            timeout_seconds: default_timeout(),
        }
    }
}

fn default_schema_path() -> PathBuf {
    PathBuf::from("schema")
}

impl Config {
    fn infer_dialect_from_url(url: &str) -> Option<SqlDialect> {
        let scheme = url.split(':').next()?.to_ascii_lowercase();
        match scheme.as_str() {
            "postgres" | "postgresql" => Some(SqlDialect::Postgres),
            "mysql" => Some(SqlDialect::Mysql),
            "sqlite" => Some(SqlDialect::Sqlite),
            _ => None,
        }
    }

    /// Load configuration from a file
    pub fn load(path: &std::path::Path, args: &CommonArgs) -> crate::Result<Self> {
        dotenvy::dotenv().ok();
        let config: Config = Figment::new()
            .merge(Serialized::defaults(Self::default()))
            .merge(Toml::file(path))
            .merge(Env::raw())
            .merge(Env::prefixed("SHKI_").split("__"))
            .merge(Serialized::defaults(args))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to load config: {}", e)))?;
        let mut config = config.infer_dialect();
        if let Some(migrations_dir) = &args.migrations_dir {
            config.out = migrations_dir.clone();
        }
        Ok(config)
    }

    pub fn with_dialect(mut self, dialect: SqlDialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// if dialect is not already set, try to infer it from the database URL
    pub fn infer_dialect(mut self) -> Self {
        if let Some(database_url) = self.database_url.as_deref()
            && let Some(dialect) = Self::infer_dialect_from_url(database_url)
        {
            self.dialect = dialect;
        }

        // Only Postgres supports defining a schema, so we ensure schema is not set
        if self.dialect != SqlDialect::Postgres {
            self.migrations.schema = None
        }

        self
    }

    /// Save configuration to a file
    pub fn save(&self, path: &std::path::Path) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ShkiError::config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the resolved output directory for migrations
    pub fn out_dir(&self) -> PathBuf {
        resolve_path(Some(self.root.clone()), &self.out)
    }

    /// Get the resolved schema path
    pub fn schema_path(&self) -> PathBuf {
        resolve_path(Some(self.root.clone()), &self.schema)
    }

    // Get the resolved codegen output directory (if configured)
    // pub fn codegen_out_dir(&self) -> Option<PathBuf> {
    //     self.codegen.output.as_ref().map(|p| self.resolve_path(p))
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_applies_file_env_and_cli_precedence() {
        let _guard = env_lock().lock().expect("failed to lock env");
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("shki.toml");

        std::fs::write(
            &config_path,
            r#"
root = "db"
dialect = "sqlite"
database_url = "sqlite://from-file.db"

[migrations]
table = "file_migrations"
prefix = "index"
generate_down = false
"#,
        )
        .expect("failed to write config");

        unsafe {
            std::env::set_var("DATABASE_URL", "sqlite://from-raw-env.db");
            std::env::set_var("SHKI_DATABASE_URL", "sqlite://from-shki-env.db");
            std::env::set_var("SHKI_SHADOW_DATABASE_POSTGRES_VERSION", "16");
            std::env::set_var("SHKI_MIGRATIONS__TABLE", "env_migrations");
        }

        let args = CommonArgs {
            dialect: Some(SqlDialect::Postgres),
            database_url: Some("postgres://from-cli".to_string()),
            pg_version: Some(17),
            migrations_dir: Some(PathBuf::from("cli-migrations")),
            migrations: crate::cli::args::MigrationArgs {
                prefix: Some(MigrationPrefix::Timestamp),
                generate_down: true,
                table: None,
                schema: None,
            },
            ..CommonArgs::default()
        };

        let config = Config::load(&config_path, &args).expect("config should load");

        assert_eq!(config.dialect, SqlDialect::Postgres);
        assert_eq!(config.database_url.as_deref(), Some("postgres://from-cli"));
        assert_eq!(config.pg_version, Some(17));
        assert_eq!(config.out, PathBuf::from("cli-migrations"));
        assert_eq!(config.migrations.entity().name, "env_migrations");
        assert_eq!(config.migrations.prefix, MigrationPrefix::Timestamp);
        assert!(config.migrations.generate_down);

        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::remove_var("SHKI_DATABASE_URL");
            std::env::remove_var("SHKI_SHADOW_DATABASE_POSTGRES_VERSION");
            std::env::remove_var("SHKI_MIGRATIONS__TABLE");
        }
    }
}
