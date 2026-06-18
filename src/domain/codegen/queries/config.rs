use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::codegen::OutputMode;

/// Configuration for Rust code generation from sql queries
#[derive(Debug, Clone, Serialize, Deserialize, Default, Args)]
pub struct QueriesConfig {
    /// Output file for generated Rust (prints to stdout if omitted)
    #[arg(short, long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,

    /// SQL file or directory of annotated *.sql query files (default: <root>/queries)
    #[arg(short = 's', long = "sources")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<PathBuf>,

    /// Output mode: single file or module directory
    #[arg(long, short)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputMode>,

    /// Rust module path to import schema types from (e.g. `crate::models`).
    /// Optional: when unset it is derived from the codegen and
    /// queries output paths (e.g. sibling `models.rs`/`queries.rs` -> `super::models`).
    /// Set this only to override that derivation for non-conventional layouts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[arg(long = "models")]
    pub models: Option<String>,
}
