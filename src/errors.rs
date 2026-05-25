use thiserror::Error;

pub type Result<T> = anyhow::Result<T, ShkiError>;

/// Details about a single checksum mismatch between a snapshot and migration file
#[derive(Debug, Clone)]
pub struct MismatchDetail {
    /// Snapshot ID
    pub snapshot_id: String,
    /// Migration name from the snapshot
    pub migration_name: String,
    /// Checksum stored in the snapshot
    pub snapshot_checksum: String,
    /// Current checksum of the migration file (None if file not found)
    pub file_checksum: Option<String>,
    /// Description of the issue
    pub issue: String,
}

/// Summary of snapshot validation failures
#[derive(Debug, Clone)]
pub struct SnapshotValidationSummary {
    /// Total number of snapshots checked
    pub total_snapshots: usize,
    /// Number of snapshots with migration info
    pub snapshots_with_migrations: usize,
    /// List of mismatches found
    pub mismatches: Vec<MismatchDetail>,
}

impl std::fmt::Display for SnapshotValidationSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n=== Snapshot Validation Summary ===")?;
        writeln!(f, "Total snapshots:        {}", self.total_snapshots)?;
        writeln!(
            f,
            "With migration info:    {}",
            self.snapshots_with_migrations
        )?;
        writeln!(f, "Mismatches found:       {}", self.mismatches.len())?;

        if !self.mismatches.is_empty() {
            writeln!(f, "\n--- Mismatch Details ---")?;
            for (i, m) in self.mismatches.iter().enumerate() {
                writeln!(f, "\n[{}] Migration: {}", i + 1, m.migration_name)?;
                writeln!(f, "    Snapshot ID:       {}...", &m.snapshot_id[..8])?;
                writeln!(
                    f,
                    "    Snapshot checksum: {}...",
                    &m.snapshot_checksum[..16]
                )?;
                if let Some(ref fc) = m.file_checksum {
                    writeln!(f, "    File checksum:     {}...", &fc[..16])?;
                } else {
                    writeln!(f, "    File checksum:     (file not found)")?;
                }
                writeln!(f, "    Issue: {}", m.issue)?;
            }
            writeln!(f, "\n--- Resolution ---")?;
            writeln!(
                f,
                "Migration files have been modified after snapshots were created."
            )?;
            writeln!(
                f,
                "This can cause inconsistencies between your schema snapshots"
            )?;
            writeln!(f, "and the actual migrations that will be applied.")?;
            writeln!(f, "\nTo resolve:")?;
            writeln!(
                f,
                "  1. Restore the original migration files from version control, OR"
            )?;
            writeln!(
                f,
                "  2. Regenerate the snapshots if the changes are intentional, OR"
            )?;
            writeln!(
                f,
                "  3. Delete the affected snapshots and migrations to start fresh"
            )?;
        }

        Ok(())
    }
}

/// Main error type for Shki
#[derive(Error, Debug)]
pub enum ShkiError {
    #[error("[Input] {0}")]
    Input(#[from] inquire::InquireError),

    #[error("[DB] {0}")]
    Database(#[from] sqlx::Error),

    #[error("[IO] {0}")]
    Io(#[from] std::io::Error),

    #[error("[JSON] {0}")]
    Json(#[from] serde_json::Error),

    #[error("[YAML] {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("[TOML] {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("[CONFIG] {0}")]
    Config(String),

    #[error("[SCHEMA] {0}")]
    Schema(String),

    #[error("[MIGRATION] {0}")]
    Migration(String),

    #[error("[CHECKSUM] Migration '{name}' checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("[SNAPSHOT-VALIDATION] Snapshot validation failed{0}")]
    SnapshotValidation(SnapshotValidationSummary),

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

    #[error("[TYPESCRIPT] {0}")]
    Typescript(String),

    #[error("[USER] Cancelled")]
    Cancelled,

    #[error("[SER] {0}")]
    Deserialize(String),
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

    pub fn checksum_mismatch(
        name: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        ShkiError::ChecksumMismatch {
            name: name.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn snapshot_validation(summary: SnapshotValidationSummary) -> Self {
        ShkiError::SnapshotValidation(summary)
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

    pub fn typescript(msg: impl Into<String>) -> Self {
        ShkiError::Typescript(msg.into())
    }
}
