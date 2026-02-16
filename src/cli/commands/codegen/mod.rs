//! Code generation from schema definitions
//!
//! This module generates code from database schema definitions,
//! supporting multiple output languages including Rust and Protocol Buffers.

mod config;
pub mod languages;
mod writer;

use colored::Colorize;
pub use config::*;

use crate::cli::CodegenLanguage;
use crate::snapshot::Snapshot;
use crate::{Config, Result};
use std::path::PathBuf;

use languages::{CodeGenerator, ProtobufWriter, RustWriter, TypeScriptWriter};
use writer::CodeWriter;

pub fn cmd_codegen(
    config: &Config,
    schema: Option<PathBuf>,
    mode: Option<OutputMode>,
    output: Option<PathBuf>,
    verbose: Option<bool>,
    language: CodegenLanguage,
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

    match language {
        CodegenLanguage::Rust => {
            let generated = languages::RustGenerator::generate(&snapshot, gen_config);
            let writer = RustWriter;

            if gen_config.verbose {
                println!("{}", writer.format_preview(&generated));
            }

            if gen_config.output.is_none() {
                println!(
                    "{}",
                    "Generation skipped: no output path specified.".yellow()
                );
                return Ok(());
            }

            writer.write(&generated, gen_config)?;
        }
        CodegenLanguage::Protobuf => {
            let generated = languages::ProtobufGenerator::generate(&snapshot, gen_config);
            let writer = ProtobufWriter;

            if gen_config.verbose {
                println!("{}", writer.format_preview(&generated));
            }

            if gen_config.output.is_none() {
                println!(
                    "{}",
                    "Generation skipped: no output path specified.".yellow()
                );
                return Ok(());
            }

            writer.write(&generated, gen_config)?;
        }
        CodegenLanguage::TypeScript => {
            let generated = languages::TypeScriptGenerator::generate(&snapshot, gen_config);
            let writer = TypeScriptWriter;

            if gen_config.verbose {
                println!("{}", writer.format_preview(&generated));
            }

            if gen_config.output.is_none() {
                println!(
                    "{}",
                    "Generation skipped: no output path specified.".yellow()
                );
                return Ok(());
            }

            writer.write(&generated, gen_config)?;
        }
    }

    Ok(())
}
