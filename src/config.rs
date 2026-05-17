//! Configuration for Shki
//!
//! This module provides configuration structures for the CLI and library.

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{CommonArgs, ShkiError, models::entity_name::EntityName, schema::SqlDialect};
use clap::ValueEnum;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Default, Deserialize)]
struct ExplicitConfigProbe {
    dialect: Option<SqlDialect>,
}

/// Schema definition language
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaMode {
    /// Just write SQL migration files without any schema definition or diffing
    #[default]
    Sql,

    /// Define schemas using Lua scripts
    Lua,
}

impl std::fmt::Display for SchemaMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaMode::Sql => write!(f, "sql"),
            SchemaMode::Lua => write!(f, "lua"),
        }
    }
}

/// Main configuration for Shki
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Project root directory
    pub root: PathBuf,

    /// Database dialect
    pub dialect: SqlDialect,

    /// Path to schema files (glob pattern)
    #[serde(default)]
    pub schema: String,

    /// Output directory for migrations
    #[serde(default = "default_out")]
    pub out: PathBuf,

    /// Database connection URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    /// Whether to add breakpoints between SQL statements
    #[serde(default = "default_true")]
    pub breakpoints: bool,

    /// Verbose output
    #[serde(default)]
    pub verbose: bool,

    /// Migration settings
    #[serde(default)]
    pub migrations: MigrationConfig,

    // Introspection settings
    // #[serde(default)]
    // pub introspect: IntrospectConfig,

    // #[serde(default)]
    // pub codegen: CodegenConfig,
    /// Database connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default)]
    pub mode: SchemaMode,
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

    /// Schema for the migrations table (PostgreSQL)
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

impl From<MigrationTableId> for EntityName {
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
    pub fn entity(&self) -> EntityName {
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

/// Introspection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntrospectConfig {
    /// Casing for generated code
    #[serde(default)]
    pub casing: IdentifierCasing,
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
            schema: "init.lua".to_string(),
            out: default_out(),
            database_url: None,
            breakpoints: true,
            verbose: false,
            // codegen: CodegenConfig::default(),
            migrations: MigrationConfig::default(),
            // introspect: IntrospectConfig::default(),
            timeout_seconds: default_timeout(),
        }
    }
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
        let config = config.infer_dialect();
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

    /// Resolve a path relative to the project root.
    ///
    /// If the path is absolute, it is returned as-is.
    /// If the path is relative, it is joined with the root directory.
    pub fn resolve_path(&self, path: impl AsRef<std::path::Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    /// Get the resolved output directory for migrations
    pub fn out_dir(&self) -> PathBuf {
        self.resolve_path(&self.out)
    }

    /// Get the resolved schema path
    pub fn schema_path(&self) -> PathBuf {
        self.resolve_path(&self.schema)
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
    fn resolve_path_uses_root_for_relative_paths_only() {
        let config = Config {
            root: PathBuf::from("/tmp/shki-root"),
            out: PathBuf::from("migrations"),
            schema: "schema/init.lua".to_string(),
            ..Config::default()
        };

        assert_eq!(config.out_dir(), PathBuf::from("/tmp/shki-root/migrations"));
        assert_eq!(
            config.schema_path(),
            PathBuf::from("/tmp/shki-root/schema/init.lua")
        );
        assert_eq!(
            config.resolve_path("/var/tmp/already-absolute.sql"),
            PathBuf::from("/var/tmp/already-absolute.sql")
        );
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
            std::env::set_var("SHKI_MIGRATIONS__TABLE", "env_migrations");
        }

        let args = CommonArgs {
            dialect: Some(SqlDialect::Postgres),
            database_url: Some("postgres://from-cli".to_string()),
            out: Some(PathBuf::from("cli-migrations")),
            migrations: crate::cli::args::MigrationArgs {
                prefix: Some(MigrationPrefix::Timestamp),
                generate_down: true,
                table: None,
                schema: None,
            },
            ..CommonArgs::default()
        };

        let config = Config::load(&config_path, &args).expect("config should load");

        assert_eq!(config.root, PathBuf::from("db"));
        assert_eq!(config.dialect, SqlDialect::Postgres);
        assert_eq!(config.database_url.as_deref(), Some("postgres://from-cli"));
        assert_eq!(config.out, PathBuf::from("cli-migrations"));
        assert_eq!(config.migrations.entity().name, "env_migrations");
        assert_eq!(config.migrations.prefix, MigrationPrefix::Timestamp);
        assert!(config.migrations.generate_down);

        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::remove_var("SHKI_DATABASE_URL");
            std::env::remove_var("SHKI_MIGRATIONS__TABLE");
        }
    }
}
