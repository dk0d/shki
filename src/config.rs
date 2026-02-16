//! Configuration for Shki
//!
//! This module provides configuration structures for the CLI and library.

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{commands::codegen::CodegenConfig, schema::SchemaDialect, ShkiError};

/// Main configuration for Shki
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Project root directory
    pub root: PathBuf,

    /// Database dialect
    pub dialect: SchemaDialect,

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

    /// Introspection settings
    #[serde(default)]
    pub introspect: IntrospectConfig,

    #[serde(default)]
    pub codegen: CodegenConfig,

    /// Database connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
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

/// Migration-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Name of the migrations table
    #[serde(default = "default_migrations_table")]
    pub table: String,

    /// Schema for the migrations table (PostgreSQL)
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            table: default_migrations_table(),
            schema: None,
            prefix: MigrationPrefix::Index,
            generate_down: false,
        }
    }
}

/// Migration file name prefix style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MigrationPrefix {
    /// Sequential index (0000, 0001, 0002, ...)
    Index,

    /// Timestamp (20240101120000)
    #[default]
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
            dialect: SchemaDialect::Postgres,
            schema: "init.lua".to_string(),
            out: default_out(),
            database_url: None,
            breakpoints: true,
            verbose: false,
            codegen: CodegenConfig::default(),
            migrations: MigrationConfig::default(),
            introspect: IntrospectConfig::default(),
            timeout_seconds: default_timeout(),
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn load(path: &std::path::Path) -> crate::Result<Self> {
        let _ = dotenvy::dotenv();
        // let content = std::fs::read_to_string(path)?;
        let config: Config = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("SHKI_").split("__"))
            .extract()
            .map_err(|e| ShkiError::config(format!("Failed to load config: {}", e)))?;
        Ok(config)
    }

    /// Load configuration from the default location (shki.toml)
    pub fn load_default() -> crate::Result<Self> {
        let path = PathBuf::from("shki.toml");
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::default())
        }
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

    /// Get the resolved codegen output directory (if configured)
    pub fn codegen_out_dir(&self) -> Option<PathBuf> {
        self.codegen.output.as_ref().map(|p| self.resolve_path(p))
    }
}
