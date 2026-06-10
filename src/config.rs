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
    CodegenArgs, CommonArgs, MigrationArgs, ShadowArgs, ShkiError, codegen::CodegenConfig,
    models::iden::Iden, schema::SqlDialect, utils::resolve_path,
};
use clap::ValueEnum;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Default, Deserialize)]
struct ExplicitConfigProbe {
    dialect: Option<SqlDialect>,
    root: Option<PathBuf>,
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
    #[serde(default)]
    pub dialect: SqlDialect,

    /// Path to schema dir/file entrypoint
    #[serde(default = "default_schema_dir")]
    pub schema: PathBuf,

    /// Output directory for migrations
    #[serde(default = "default_out", alias = "out")]
    pub migrations_dir: PathBuf,

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

fn default_schema_dir() -> PathBuf {
    PathBuf::from("schema")
}

fn default_false() -> bool {
    false
}

fn default_timeout() -> u64 {
    2
}

fn default_root() -> PathBuf {
    std::env::current_dir().unwrap_or(PathBuf::from("./"))
}

fn default_out() -> PathBuf {
    PathBuf::from("migrations")
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
            migrations_dir: default_out(),
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

#[derive(Debug, Clone, Serialize)]
struct MigrationOverride {
    #[serde(skip_serializing_if = "MigrationArgs::is_empty")]
    migrations: MigrationArgs,
}

#[derive(Debug, Clone, Serialize)]
struct CodegenOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    codegen: Option<CodegenConfigOverride>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct CodegenConfigOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<crate::codegen::OutputMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    serde: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sqlx: Option<bool>,
}

impl From<&CodegenArgs> for CodegenOverride {
    fn from(args: &CodegenArgs) -> Self {
        if args.is_empty() {
            return Self { codegen: None };
        }

        Self {
            codegen: Some(CodegenConfigOverride {
                output: args.output.clone(),
                format: args.format,
                serde: args.serde.then_some(true),
                sqlx: if args.no_sqlx {
                    Some(false)
                } else {
                    args.sqlx.then_some(true)
                },
            }),
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
        let explicit = Self::explicit_config(path)?;
        let config: Config = Self::base_figment(path)
            .merge(Serialized::defaults(args))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to load config: {}", e)))?;
        let mut config = config.infer_dialect();
        if explicit.root.is_none()
            && let Some(parent) = path.parent()
        {
            config.root = parent.to_path_buf();
        }
        if let Some(migrations_dir) = &args.migrations_dir {
            config.migrations_dir = migrations_dir.clone();
        }
        Ok(config)
    }

    fn explicit_config(path: &std::path::Path) -> crate::Result<ExplicitConfigProbe> {
        Figment::new()
            .merge(Toml::file(path))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to inspect config: {}", e)))
    }

    fn base_figment(path: &std::path::Path) -> Figment {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::raw())
            .merge(Env::prefixed("SHKI_").split("__"))
    }

    pub fn with_shadow_args(mut self, args: &ShadowArgs) -> crate::Result<Self> {
        self = Figment::from(Serialized::defaults(self))
            .merge(Serialized::defaults(args))
            .extract()
            .map_err(|e| {
                ShkiError::config(format!("Failed to apply Shadow Database args: {}", e))
            })?;
        Ok(self.infer_dialect())
    }

    pub fn with_migration_args(mut self, args: &MigrationArgs) -> crate::Result<Self> {
        self = Figment::from(Serialized::defaults(self))
            .merge(Serialized::defaults(MigrationOverride {
                migrations: args.clone(),
            }))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to apply migration args: {}", e)))?;
        Ok(self.infer_dialect())
    }

    pub fn with_codegen_args(mut self, args: &CodegenArgs) -> crate::Result<Self> {
        self = Figment::from(Serialized::defaults(self))
            .merge(Serialized::defaults(CodegenOverride::from(args)))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to apply codegen args: {}", e)))?;
        Ok(self.infer_dialect())
    }

    pub fn with_command_args(
        self,
        shadow: Option<&ShadowArgs>,
        migrations: Option<&MigrationArgs>,
        codegen: Option<&CodegenArgs>,
    ) -> crate::Result<Self> {
        let mut config = self;
        if let Some(shadow) = shadow {
            config = config.with_shadow_args(shadow)?;
        }
        if let Some(migrations) = migrations {
            config = config.with_migration_args(migrations)?;
        }
        if let Some(codegen) = codegen {
            config = config.with_codegen_args(codegen)?;
        }
        Ok(config)
    }

    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }

    pub fn with_dialect(mut self, dialect: SqlDialect) -> Self {
        self.dialect = dialect;
        self
    }

    pub fn require_database_url(&self) -> crate::Result<&str> {
        self.database_url
            .as_deref()
            .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))
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
        resolve_path(Some(self.root.clone()), &self.migrations_dir)
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

        let common = CommonArgs {
            dialect: Some(SqlDialect::Postgres),
            database_url: Some("postgres://from-cli".to_string()),
            migrations_dir: Some(PathBuf::from("cli-migrations")),
            ..CommonArgs::default()
        };

        let config = Config::load(&config_path, &common)
            .expect("config should load")
            .with_shadow_args(&ShadowArgs {
                pg_version: Some(17),
                shadow_database_url: None,
            })
            .expect("shadow args should apply")
            .with_migration_args(&MigrationArgs {
                prefix: Some(MigrationPrefix::Timestamp),
                generate_down: true,
                table: None,
            })
            .expect("migration args should apply");

        assert_eq!(config.dialect, SqlDialect::Postgres);
        assert_eq!(config.database_url.as_deref(), Some("postgres://from-cli"));
        assert_eq!(config.pg_version, Some(17));
        assert_eq!(config.migrations_dir, PathBuf::from("cli-migrations"));
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

    #[test]
    fn load_does_not_apply_command_scoped_default_overrides() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("shki.toml");

        std::fs::write(
            &config_path,
            r#"
dialect = "postgres"

[codegen]
sqlx = false

[migrations]
generate_down = false
"#,
        )
        .expect("failed to write config");

        let config =
            Config::load(&config_path, &CommonArgs::default()).expect("config should load");

        assert!(!config.codegen.sqlx);
        assert!(!config.migrations.generate_down);
    }

    #[test]
    fn command_scoped_overrides_apply_through_figment() {
        let config = Config {
            codegen: CodegenConfig {
                sqlx: false,
                ..CodegenConfig::default()
            },
            migrations: MigrationConfig {
                generate_down: false,
                ..MigrationConfig::default()
            },
            ..Config::default()
        }
        .with_codegen_args(&CodegenArgs {
            sqlx: true,
            ..CodegenArgs::default()
        })
        .expect("codegen args should apply")
        .with_migration_args(&MigrationArgs {
            generate_down: true,
            ..MigrationArgs::default()
        })
        .expect("migration args should apply");

        assert!(config.codegen.sqlx);
        assert!(config.migrations.generate_down);
    }

    #[test]
    fn codegen_no_sqlx_override_sets_false() {
        let config = Config {
            codegen: CodegenConfig {
                sqlx: true,
                ..CodegenConfig::default()
            },
            ..Config::default()
        }
        .with_codegen_args(&CodegenArgs {
            no_sqlx: true,
            ..CodegenArgs::default()
        })
        .expect("codegen args should apply");

        assert!(!config.codegen.sqlx);
    }

    #[test]
    fn load_accepts_out_alias_for_migrations_dir() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("shki.toml");

        std::fs::write(
            &config_path,
            format!(
                r#"
root = "{}"
dialect = "sqlite"
out = "db/migrations"
"#,
                temp_dir.path().display()
            ),
        )
        .expect("failed to write config");

        let config =
            Config::load(&config_path, &CommonArgs::default()).expect("config should load");

        assert_eq!(config.migrations_dir, PathBuf::from("db/migrations"));
        assert_eq!(config.out_dir(), temp_dir.path().join("db/migrations"));
    }

    #[test]
    fn load_defaults_root_to_config_file_parent() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("shki.toml");

        std::fs::write(
            &config_path,
            r#"
dialect = "sqlite"
database_url = "sqlite://test.db"
"#,
        )
        .expect("failed to write config");

        let config =
            Config::load(&config_path, &CommonArgs::default()).expect("config should load");

        assert_eq!(config.root, temp_dir.path());
        assert_eq!(config.schema_path(), temp_dir.path().join("schema"));
        assert_eq!(config.out_dir(), temp_dir.path().join("migrations"));
    }

    #[test]
    fn default_migrations_dir_is_resolved_from_root() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config = Config {
            root: temp_dir.path().to_path_buf(),
            ..Config::default()
        };

        assert_eq!(config.migrations_dir, PathBuf::from("migrations"));
        assert_eq!(config.out_dir(), temp_dir.path().join("migrations"));
    }
}
