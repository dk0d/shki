//! Configuration for Shki
//!
//! This module provides configuration structures for the CLI and library.

use figment::{
    Figment, Provider,
    providers::{Env, Format, Serialized, Toml},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{
    CodegenArgs, CommonArgs, MigrationArgs, ShadowArgs, ShkiError, codegen::CodegenConfig,
    models::iden::Iden, schema::SqlDialect, utils::resolve_path,
};
#[cfg(feature = "querygen")]
use crate::{QueriesArgs, codegen::queries::QueriesConfig};
use clap::ValueEnum;
use colored::Colorize;

const PROJECT_ROOT_MARKERS: [&str; 2] = ["shki.toml", ".git"];

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

    /// Global CLI-overridable fields (dialect, database_url, migrations_dir,
    /// verbose, no_color). Flattened so they stay top-level config keys and share
    /// one definition with the global CLI flags. Read the resolved values via the
    /// accessors (`dialect()`, `migrations_dir()`, `database_url()`, ...).
    #[serde(flatten)]
    pub common: CommonArgs,

    /// Path to schema dir/file entrypoint
    #[serde(default = "default_schema_dir")]
    pub schema: PathBuf,

    /// Shadow Database overrides (URL + embedded PostgreSQL version). Flattened
    /// so `shadow_database_url`/`pg_version` stay top-level config/env keys, and
    /// the `--shadow-database-url`/`--pg-version` flags share this definition.
    #[serde(flatten)]
    pub shadow: ShadowArgs,

    /// Whether to add breakpoints between SQL statements
    #[serde(default = "default_true")]
    pub breakpoints: bool,

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

    #[cfg(feature = "querygen")]
    #[serde(default)]
    pub queries: QueriesConfig,
}

fn default_schema_dir() -> PathBuf {
    PathBuf::from("schema")
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

/// Migration-specific configuration.
///
/// The CLI-overridable fields (`table`, `prefix`, `generate_down`) live in the
/// flattened [`MigrationArgs`] so they have a single definition shared with the
/// command-line flags; `schema` is config-only. Read the resolved values via
/// the accessors (`table()`, `prefix()`, `schema`), which apply defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// CLI-overridable migration fields (also the migration subcommand flags).
    #[serde(flatten)]
    pub args: MigrationArgs,

    /// Schema for the migrations table (PostgreSQL)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_migrations_schema"
    )]
    pub schema: Option<String>,
}

fn default_migrations_table() -> String {
    "__shki_migrations".to_string()
}

fn default_migrations_schema() -> Option<String> {
    "shki".to_string().into()
}

impl MigrationConfig {
    /// Resolved migrations table name (defaults to `__shki_migrations`).
    pub fn table(&self) -> String {
        self.args
            .table
            .clone()
            .unwrap_or_else(default_migrations_table)
    }

    /// Resolved migration file prefix style (defaults to `Index`).
    pub fn prefix(&self) -> MigrationPrefix {
        self.args.prefix.unwrap_or_default()
    }

    /// Whether to also generate down migrations.
    pub fn generate_down(&self) -> bool {
        self.args.generate_down
    }

    pub fn entity(&self) -> Iden {
        (self.table(), self.schema.clone()).into()
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            args: MigrationArgs::default(),
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
            common: CommonArgs::default(),
            mode: SchemaMode::default(),
            schema: default_schema_path(),
            shadow: ShadowArgs::default(),
            breakpoints: true,
            codegen: CodegenConfig::default(),
            migrations: MigrationConfig::default(),
            // introspect: IntrospectConfig::default(),
            timeout_seconds: default_timeout(),
            #[cfg(feature = "querygen")]
            queries: QueriesConfig::default(),
        }
    }
}

fn default_schema_path() -> PathBuf {
    PathBuf::from("schema")
}

fn sanitize_db_url(url: &str) -> String {
    let pattern = Regex::new(r":([^:@]+)@").unwrap();
    let masked_url = pattern.replace(url, ":********@");
    format!("{}", masked_url)
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

    /// Resolved database dialect (defaults to the [`SqlDialect`] default; usually
    /// inferred from the database URL by [`infer_dialect`](Self::infer_dialect)).
    pub fn dialect(&self) -> SqlDialect {
        self.common.dialect.unwrap_or_default()
    }

    /// Resolved migrations directory (defaults to `migrations`).
    pub fn migrations_dir(&self) -> PathBuf {
        self.common
            .migrations_dir
            .clone()
            .unwrap_or_else(default_out)
    }

    /// Configured database connection URL, if any.
    pub fn database_url(&self) -> Option<&str> {
        self.common.database_url.as_deref()
    }

    /// Whether verbose output is enabled.
    pub fn verbose(&self) -> bool {
        self.common.verbose
    }

    /// Whether colored output is disabled.
    pub fn no_color(&self) -> bool {
        self.common.no_color
    }

    pub fn display_sanitized_db_url(&self) {
        if let Some(url) = self.database_url() {
            let sanitized_url = sanitize_db_url(url);
            println!("\n{} {}\n", "URL".bold(), sanitized_url.bright_green());
        } else {
            println!("{}", "No database url found".bright_yellow());
        }
    }

    /// Load configuration from a file
    pub fn load(path: &std::path::Path, args: &CommonArgs) -> crate::Result<Self> {
        dotenvy::dotenv().ok();
        let cwd = std::env::current_dir().expect("working dir");
        let root = resolve_project_root(path, &cwd);
        let path = root.join("shki.toml");
        let explicit = Self::explicit_config(&path)?;
        let config: Config = Self::base_figment(&path)
            .merge(Serialized::defaults(args))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to load config: {}", e)))?;
        let mut config = config.infer_dialect();
        if explicit.root.is_none() {
            config.root = root;
        }
        // `--schema`/`-S` is a global flag that targets the *migration table*
        // schema (`migrations.schema`), so it is applied here rather than merged
        // into the top-level `schema` (the schema-dir) key.
        if let Some(schema) = &args.schema {
            config.migrations.schema = Some(schema.clone());
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

    pub fn with_shadow_args(self, args: &ShadowArgs) -> crate::Result<Self> {
        self.merge_args(Serialized::defaults(args))
    }

    pub fn with_migration_args(self, args: &MigrationArgs) -> crate::Result<Self> {
        self.merge_args(Serialized::default("migrations", args))
    }

    pub fn with_codegen_args(self, args: &CodegenArgs) -> crate::Result<Self> {
        self.merge_args(Serialized::default("codegen", args))
    }

    #[cfg(feature = "querygen")]
    pub fn with_querygen_args(self, args: &QueriesArgs) -> crate::Result<Self> {
        self.merge_args(Serialized::default("queries", args))
    }

    /// Merge a CLI-args provider over the current config, letting figment
    /// deep-merge only the fields the user actually set (args skip absent
    /// values when serialized). Each `with_*_args` helper picks the key the
    /// args nest under; the shape of the args mirrors the config it overrides.
    fn merge_args<P: Provider>(self, args: P) -> crate::Result<Self> {
        let merged: Config = Figment::from(Serialized::defaults(self))
            .merge(args)
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to apply args: {}", e)))?;
        Ok(merged.infer_dialect())
    }

    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }

    pub fn with_dialect(mut self, dialect: SqlDialect) -> Self {
        self.common.dialect = Some(dialect);
        self
    }

    pub fn require_database_url(&self) -> crate::Result<&str> {
        self.database_url()
            .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))
    }

    /// if dialect is not already set, try to infer it from the database URL
    pub fn infer_dialect(mut self) -> Self {
        if let Some(database_url) = self.common.database_url.as_deref()
            && let Some(dialect) = Self::infer_dialect_from_url(database_url)
        {
            self.common.dialect = Some(dialect);
        }

        // Only Postgres supports defining a schema, so we ensure schema is not set
        if self.dialect() != SqlDialect::Postgres {
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
        resolve_path(Some(self.root.clone()), self.migrations_dir())
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

fn resolve_project_root(starting_path: &Path, default_root: &Path) -> PathBuf {
    let default = std::fs::canonicalize(default_root).expect("default");
    let starting_path = std::fs::canonicalize(starting_path).unwrap_or(default.clone());

    if starting_path.is_file()
        && starting_path
            .file_name()
            .is_some_and(|p| p.to_string_lossy() == "shki.toml")
        && starting_path.exists()
        && let Some(parent) = starting_path.parent()
    {
        return parent.to_path_buf();
    }

    let starting_path = starting_path.as_path();

    let parent = if starting_path.is_file() {
        starting_path.ancestors().nth(2)
    } else if starting_path.is_dir() {
        starting_path.ancestors().nth(1)
    } else {
        Some(starting_path)
    };

    let mut search_dir = if let Some(path) = parent
        && path.to_string_lossy() != ""
    {
        Some(path)
    } else {
        Some(default.as_path())
    };

    while let Some(path) = search_dir {
        if PROJECT_ROOT_MARKERS
            .iter()
            .find(|n| path.join(n).exists())
            .is_some()
        {
            return path.to_path_buf();
        }
        search_dir = path.parent();
    }
    default_root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::create_dir_all;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    #[test]
    fn test_db_url_is_sanitized() {
        let url = "postgresql://pg:must_not_be_shown@localhost:5432/app".to_string();
        let sanitized = sanitize_db_url(&url);
        assert!(sanitized.find("must_not_be_shown").is_none())
    }

    #[test]
    fn test_resolve_project_config_file() {
        let root = TempDir::new().expect("failed temp dir create");
        let target_config = root.path().join("shki.toml");
        std::fs::write(&target_config, "# config").expect("write to file");
        // cwd
        let good_starting = root
            .path()
            .join("some")
            .join("deeply")
            .join("nested")
            .join("place");

        create_dir_all(&good_starting).expect("make directory");
        let expected = std::fs::canonicalize(root.path()).unwrap();

        let found = resolve_project_root(&good_starting, &good_starting);
        assert_eq!(found, expected);

        // exact start
        let exact_start = target_config.clone();
        let found = resolve_project_root(&exact_start, root.path());
        assert_eq!(found, expected);

        // from not found file
        let file_starting = root
            .path()
            .join("some")
            .join("deeply")
            .join("nested")
            .join("other.toml");
        let found = resolve_project_root(&file_starting, file_starting.parent().unwrap());
        assert_eq!(found, expected);
    }

    #[test]
    fn test_resolve_project_config_file_exit_at_git() {
        let root = TempDir::new().expect("failed temp dir create");
        let good_starting = root
            .path()
            .join("some")
            .join(".git")
            .join("nested")
            .join("place");
        create_dir_all(&good_starting).expect("make directory");
        let expected = std::fs::canonicalize(root.path().join("some")).unwrap();
        let found = resolve_project_root(&good_starting, &good_starting);
        assert_eq!(found, expected)
    }

    #[test]
    fn test_resolve_project_config_file_empty() {
        let root = TempDir::new().expect("failed temp dir create");
        let target = root.path().join("some");
        let good_starting = target.join(".git").join("nested").join("place");
        create_dir_all(&good_starting).expect("make directory");
        let expected = std::fs::canonicalize(target).unwrap();
        let found = resolve_project_root(&PathBuf::new(), &good_starting);
        assert_eq!(found, expected)
    }

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

        assert_eq!(config.dialect(), SqlDialect::Postgres);
        assert_eq!(config.database_url(), Some("postgres://from-cli"));
        assert_eq!(config.shadow.pg_version, Some(17));
        assert_eq!(config.migrations_dir(), PathBuf::from("cli-migrations"));
        assert_eq!(config.migrations.entity().name, "env_migrations");
        assert_eq!(config.migrations.prefix(), MigrationPrefix::Timestamp);
        assert!(config.migrations.generate_down());

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

        assert!(!config.codegen.sqlx());
        assert!(!config.migrations.generate_down());
    }

    #[test]
    fn command_scoped_overrides_apply_through_figment() {
        let config = Config {
            codegen: CodegenConfig {
                sqlx: Some(false),
                ..CodegenConfig::default()
            },
            migrations: MigrationConfig {
                args: MigrationArgs {
                    generate_down: false,
                    ..MigrationArgs::default()
                },
                ..MigrationConfig::default()
            },
            ..Config::default()
        }
        .with_codegen_args(&CodegenArgs {
            config: CodegenConfig {
                sqlx: Some(true),
                ..CodegenConfig::default()
            },
            ..CodegenArgs::default()
        })
        .expect("codegen args should apply")
        .with_migration_args(&MigrationArgs {
            generate_down: true,
            ..MigrationArgs::default()
        })
        .expect("migration args should apply");

        assert!(config.codegen.sqlx());
        assert!(config.migrations.generate_down());
    }

    #[cfg(feature = "querygen")]
    #[test]
    fn querygen_args_override_queries_section() {
        let config = Config {
            queries: QueriesConfig {
                sources: Some(PathBuf::from("from-config")),
                models: Some("crate::models".to_string()),
                ..QueriesConfig::default()
            },
            ..Config::default()
        }
        .with_querygen_args(&QueriesArgs {
            config: QueriesConfig {
                output: Some(PathBuf::from("from-cli.rs")),
                ..QueriesConfig::default()
            },
            // CLI-only: must NOT leak into the merged config section.
            preview: true,
        })
        .expect("querygen args should apply");

        // CLI-set fields override...
        assert_eq!(config.queries.output, Some(PathBuf::from("from-cli.rs")));
        // ...while unset CLI fields leave the config-file values intact.
        assert_eq!(config.queries.sources, Some(PathBuf::from("from-config")));
        assert_eq!(config.queries.models.as_deref(), Some("crate::models"));
    }

    #[test]
    fn codegen_sqlx_false_override_disables_sqlx() {
        let config = Config {
            codegen: CodegenConfig {
                sqlx: Some(true),
                ..CodegenConfig::default()
            },
            ..Config::default()
        }
        .with_codegen_args(&CodegenArgs {
            config: CodegenConfig {
                sqlx: Some(false),
                ..CodegenConfig::default()
            },
            ..CodegenArgs::default()
        })
        .expect("codegen args should apply");

        assert!(!config.codegen.sqlx());
    }

    #[test]
    fn codegen_unset_sqlx_keeps_config_value() {
        let config = Config {
            codegen: CodegenConfig {
                sqlx: Some(false),
                ..CodegenConfig::default()
            },
            ..Config::default()
        }
        .with_codegen_args(&CodegenArgs::default())
        .expect("codegen args should apply");

        // No --sqlx flag passed: the config-file value must survive.
        assert!(!config.codegen.sqlx());
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

        assert_eq!(config.migrations_dir(), PathBuf::from("db/migrations"));
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
        let expected = std::fs::canonicalize(temp_dir.path()).expect("temp dir path");
        assert_eq!(config.root, expected);
        assert_eq!(config.schema_path(), expected.join("schema"));
        assert_eq!(config.out_dir(), expected.join("migrations"));
    }

    #[test]
    fn default_migrations_dir_is_resolved_from_root() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config = Config {
            root: temp_dir.path().to_path_buf(),
            ..Config::default()
        };

        assert_eq!(config.migrations_dir(), PathBuf::from("migrations"));
        assert_eq!(config.out_dir(), temp_dir.path().join("migrations"));
    }
}
