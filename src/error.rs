use thiserror::Error;

pub type Result<T> = anyhow::Result<T, ShkiError>;

/// Main error type for Shki
#[derive(Error, Debug)]
pub enum ShkiError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Diff error: {0}")]
    Diff(String),

    #[error("Introspection error: {0}")]
    Introspection(String),

    #[error("SQL generation error: {0}")]
    SqlGeneration(String),

    #[error("Unsupported dialect: {0}")]
    UnsupportedDialect(String),

    #[error("Ambiguous rename detected: {0}")]
    AmbiguousRename(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Lua error: {0}")]
    Lua(String),
}

impl ShkiError {
    pub fn config(msg: impl Into<String>) -> Self {
        ShkiError::Config(msg.into())
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
