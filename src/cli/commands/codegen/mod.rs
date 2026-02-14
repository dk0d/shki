//! Rust code generation from schema definitions
//!
//! This module generates Rust structs and enums from database schema definitions,
//! compatible with sqlx and other database libraries.

mod config;
mod generate;
mod types;
mod writer;

use colored::Colorize;
pub use config::*;
pub use types::*;

use crate::snapshot::Snapshot;
use crate::{Config, Result};
use std::path::PathBuf;

pub fn cmd_codegen(
    config: &Config,
    schema: Option<PathBuf>,
    mode: Option<OutputMode>,
    output: Option<PathBuf>,
    verbose: Option<bool>,
) -> Result<()> {
    let snapshot = match schema {
        Some(schema_path) => Snapshot::from_path(&schema_path)?,
        None => Snapshot::from_config(config)?,
    };

    let gen_config = &config
        .codegen
        .clone()
        .mode(mode)
        .verbose(verbose)
        .output_dir(output);
    
    let generated = generate::generate_rust_code(&snapshot, gen_config).unwrap();

    if gen_config.verbose {
        println!("{}", writer::format_generated_code(&generated));
    }

    if gen_config.output.is_none() {
        println!(
            "{}",
            "Generation skipped: no output path specified.".yellow()
        );
        return Ok(());
    }

    writer::write_generated_code(&generated, gen_config)?;
    Ok(())
}
