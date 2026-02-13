use thiserror::Error;

pub type Result<T> = anyhow::Result<T, ShkiError>;

/// Main error type for Shki
#[derive(Error, Debug)]
pub enum ShkiError {
    #[error("[DB] {0}")]
    Database(#[from] sqlx::Error),

    #[error("[IO] {0}")]
    Io(#[from] std::io::Error),

    #[error("[JSON] {0}")]
    Json(#[from] serde_json::Error),

    #[error("[TOML] {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("[CONFIG] {0}")]
    Config(String),

    #[error("[SCHEMA] {0}")]
    Schema(String),

    #[error("[MIGRATION] {0}")]
    Migration(String),

    #[error("[DIFF] {0}")]
    Diff(String),

    #[error("[INSPECT] {0}")]
    Introspection(String),

    #[error("[SQL-GEN] {0}")]
    SqlGeneration(String),

    #[error("[DIALECT] {0}")]
    UnsupportedDialect(String),

    #[error("[AMBIGUOUS-RENAME] {0}")]
    AmbiguousRename(String),

    #[error("[CONNECTION] {0}")]
    Connection(String),

    #[error("[VALIDATION] {0}")]
    Validation(String),

    #[error("[LUA] {0}")]
    Lua(String),
}

impl ShkiError {
    pub fn config(msg: impl Into<String>) -> Self {
        ShkiError::Config(msg.into())
    }

    pub fn database(e: sqlx::Error) -> Self {
        ShkiError::Database(e)
    }

    pub fn schema(msg: impl Into<String>) -> Self {
        ShkiError::Schema(msg.into())
    }

    pub fn migration(msg: impl Into<String>) -> Self {
        ShkiError::Migration(msg.into())
    }

    pub fn diff(msg: impl Into<String>) -> Self {
        ShkiError::Diff(msg.into())
    }

    pub fn introspection(msg: impl Into<String>) -> Self {
        ShkiError::Introspection(msg.into())
    }

    pub fn sql_generation(msg: impl Into<String>) -> Self {
        ShkiError::SqlGeneration(msg.into())
    }

    pub fn unsupported_dialect(dialect: impl Into<String>) -> Self {
        ShkiError::UnsupportedDialect(dialect.into())
    }

    pub fn ambiguous_rename(msg: impl Into<String>) -> Self {
        ShkiError::AmbiguousRename(msg.into())
    }

    pub fn connection(msg: impl Into<String>) -> Self {
        ShkiError::Connection(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        ShkiError::Validation(msg.into())
    }

    pub fn lua(msg: impl Into<String>) -> Self {
        ShkiError::Lua(msg.into())
    }
}
