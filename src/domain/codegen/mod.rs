pub mod config;
pub use config::*;
pub mod generator;
pub mod lang;
pub mod writer;

use std::path::PathBuf;

use crate::cli::CodegenLanguage;
use crate::codegen::generator::CodeGenerator;
use crate::codegen::lang::protobuf::{ProtobufGenerator, ProtobufWriter};
use crate::codegen::lang::rust::{RustGenerator, RustWriter};
use crate::codegen::lang::typescript::{TypeScriptGenerator, TypeScriptWriter};
use crate::codegen::writer::CodeWriter;
use crate::config::Config;
use crate::snapshots::Snapshot;
use crate::utils::resolve_path;
use crate::{Result, ShkiError};

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
        println!("Generation skipped: no output path specified.");
        return Ok(());
    }

    writer.write(&generated, config)?;
    Ok(())
}

pub fn cmd_codegen(
    config: &Config,
    schema: Option<PathBuf>,
    mode: Option<OutputMode>,
    out: Option<PathBuf>,
    verbose: Option<bool>,
    language: CodegenLanguage,
) -> Result<()> {
    let schema_path = resolve_path(
        Some(config.root.clone()),
        schema.unwrap_or(config.schema.clone()),
    );
    let content = std::fs::read_to_string(&schema_path)?;
    let snapshot: Snapshot = serde_json::from_str(&content)
        .map_err(|e| ShkiError::config(format!("Failed to parse schema JSON: {}", e)))?;

    let output = out
        .or_else(|| config.codegen.output.clone())
        .map(|output| resolve_path(Some(config.root.clone()), output));

    let gen_config = &config
        .codegen
        .clone()
        .mode(mode)
        .verbose(verbose)
        .output_dir(output);

    match language {
        CodegenLanguage::Rust => {
            run_codegen(RustGenerator::new(), RustWriter, &snapshot, gen_config)
        }
        CodegenLanguage::Protobuf => run_codegen(
            ProtobufGenerator::new(),
            ProtobufWriter,
            &snapshot,
            gen_config,
        ),
        CodegenLanguage::Typescript { flavor } => run_codegen(
            TypeScriptGenerator::new(flavor),
            TypeScriptWriter,
            &snapshot,
            gen_config,
        ),
    }
}
