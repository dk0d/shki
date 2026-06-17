pub mod config;
pub use config::*;
pub mod generator;
pub mod lang;
pub mod queries;
pub mod writer;

use std::path::PathBuf;

use crate::Result;
use crate::cli::CodegenLanguage;
use crate::codegen::generator::CodeGenerator;
use crate::codegen::lang::protobuf::{ProtobufGenerator, ProtobufWriter};
use crate::codegen::lang::rust::{RustGenerator, RustWriter};
use crate::codegen::lang::typescript::{TypeScriptGenerator, TypeScriptWriter};
use crate::codegen::writer::CodeWriter;
use crate::compiler::compiler_from_config;
use crate::config::Config;
use crate::snapshots::Snapshot;
use crate::utils::resolve_path;

fn run_codegen<G, W>(
    generator: G,
    writer: W,
    snapshot: &Snapshot,
    config: &CodegenConfig,
    preview: bool,
    no_color: bool,
) -> Result<()>
where
    G: CodeGenerator,
    W: CodeWriter<GeneratedCode = G::Output>,
{
    let generated = generator.generate(snapshot, config);

    if preview {
        println!("{}", writer.format_preview(&generated, config, no_color));
        return Ok(());
    }

    if config.output.is_none() {
        println!("Generation skipped: no output path specified.");
        return Ok(());
    }

    writer.write(&generated, config)?;
    Ok(())
}

pub async fn cmd_codegen(
    config: &Config,
    source: Option<PathBuf>,
    language: CodegenLanguage,
) -> Result<()> {
    let snapshot = load_codegen_snapshot(config, source).await?;

    let output = config
        .codegen
        .output
        .clone()
        .map(|output| resolve_path(Some(config.root.clone()), output));

    let gen_config = &config.codegen.clone().output_dir(output);

    match language {
        CodegenLanguage::Rust => run_codegen(
            RustGenerator::new(),
            RustWriter,
            &snapshot,
            gen_config,
            config.codegen.preview,
            config.no_color,
        ),
        CodegenLanguage::Protobuf => run_codegen(
            ProtobufGenerator::new(),
            ProtobufWriter,
            &snapshot,
            gen_config,
            config.codegen.preview,
            config.no_color,
        ),
        CodegenLanguage::Typescript { flavor } => run_codegen(
            TypeScriptGenerator::new(flavor),
            TypeScriptWriter,
            &snapshot,
            gen_config,
            config.codegen.preview,
            config.no_color,
        ),
    }
}

async fn load_codegen_snapshot(config: &Config, source: Option<PathBuf>) -> Result<Snapshot> {
    if let Some(source) = source {
        // let source = source.unwrap_or(config.schema.clone());
        let schema_path = resolve_path(Some(config.root.clone()), source);
        if schema_path.is_file() {
            let content = std::fs::read_to_string(&schema_path)?;
            if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) {
                return Ok(snapshot);
            }
        }

        let mut compile_config = config.clone();
        compile_config.schema = schema_path;
        return compiler_from_config(&compile_config)?
            .compile(&compile_config)
            .await;
    }

    compiler_from_config(config)?.compile(config).await
}
