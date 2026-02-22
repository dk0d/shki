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

use languages::{CodeGenerator, ProtobufWriter, RustWriter};
use writer::CodeWriter;

use self::languages::TypeScriptWriter;

fn run_codegen<G, W>(
    generator: G,
    writer: W,
    snapshot: &Snapshot,
    config: &CodegenConfig,
) -> Result<()>
where
    G: CodeGenerator,
    W: CodeWriter<GeneratedCode = G::Output>,
{
    let generated = generator.generate(snapshot, config);

    if config.verbose {
        println!("{}", writer.format_preview(&generated));
    }

    if config.output.is_none() {
        println!(
            "{}",
            "Generation skipped: no output path specified.".yellow()
        );
        return Ok(());
    }

    writer.write(&generated, config)?;
    Ok(())
}

pub fn cmd_codegen(
    config: &Config,
    schema: Option<PathBuf>,
    mode: Option<OutputMode>,
    output: Option<PathBuf>,
    verbose: Option<bool>,
    language: CodegenLanguage,
) -> Result<()> {
    // Resolve schema path relative to project root if provided
    let snapshot = match schema {
        Some(schema_path) => {
            let resolved_schema = config.resolve_path(&schema_path);
            Snapshot::from_path(&resolved_schema)?
        }
        None => Snapshot::from_config(config)?,
    };

    // Resolve output path: CLI arg > config value, both resolved relative to root
    let resolved_output = output
        .map(|p| config.resolve_path(&p))
        .or_else(|| config.codegen_out_dir());

    let gen_config = &config
        .codegen
        .clone()
        .mode(mode)
        .verbose(verbose)
        .output_dir(resolved_output);

    match language {
        CodegenLanguage::Rust => {
            run_codegen(
                languages::RustGenerator::new(),
                RustWriter,
                &snapshot,
                gen_config,
            )?;
        }
        CodegenLanguage::Protobuf => {
            run_codegen(
                languages::ProtobufGenerator::new(),
                ProtobufWriter,
                &snapshot,
                gen_config,
            )?;
        }
        CodegenLanguage::Typescript { flavor } => {
            run_codegen(
                languages::TypeScriptGenerator::new(flavor),
                TypeScriptWriter::new(flavor),
                &snapshot,
                gen_config,
            )?;
        }
    }

    Ok(())
}
