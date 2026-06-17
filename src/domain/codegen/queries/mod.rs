//! Typed query codegen.
//!
//! Reads annotated `*.sql` files, describes each query against the compiled
//! Shadow Database to resolve parameter and result types, and generates
//! type-safe Rust functions that wrap them with sqlx. See
//! `docs/adr/0001-typed-query-codegen.md`.

pub mod describe;
pub mod generator;
pub mod parse;
pub mod rewrite;

use std::path::PathBuf;

use crate::Result;
use crate::compiler::with_compiled_shadow;
use crate::config::Config;
use crate::display::preview::{PreviewFile, render_preview};
use crate::utils::resolve_path;

use describe::describe_query;
use generator::generate_rust_module;
use parse::parse_query_dir;

/// Generate typed query wrappers from a directory of annotated SQL files.
pub async fn cmd_query_codegen(
    config: &Config,
    queries_dir: Option<PathBuf>,
    output: Option<PathBuf>,
    models_module: Option<String>,
    preview: bool,
) -> Result<()> {
    let dir = queries_dir
        .map(|d| resolve_path(Some(config.root.clone()), d))
        .unwrap_or_else(|| config.root.join("queries"));

    let specs = parse_query_dir(&dir)?;
    if specs.is_empty() {
        println!("No queries found in {}", dir.display());
        return Ok(());
    }
    let query_count = specs.len();

    let codegen_config = config.codegen.clone();
    let rendered = with_compiled_shadow(config, |snapshot, pool| async move {
        let mut described = Vec::with_capacity(specs.len());
        for spec in specs {
            described.push(describe_query(&pool, &snapshot, spec).await?);
        }
        Ok(generate_rust_module(
            &described,
            &snapshot,
            &codegen_config,
            models_module.as_deref(),
        ))
    })
    .await?;

    // The file name the output would have on disk; also used as the preview's
    // file label so the preview mirrors the real layout.
    let preview_filename = output
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "queries.rs".to_string());

    match output {
        Some(output) if !preview => {
            let output_path = resolve_path(Some(config.root.clone()), output);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, rendered)?;
            println!(
                "Wrote {} typed quer{} to {}",
                query_count,
                if query_count == 1 { "y" } else { "ies" },
                output_path.display()
            );
        }
        // Preview (or no output path): render through the shared PreviewFile
        // machinery — syntax-highlighted, no files written. Useful for eyeballing
        // the result and for compiling the full output without touching the
        // project tree.
        _ => {
            let file = PreviewFile::new(preview_filename, rendered);
            println!("{}", render_preview(&[file], "rust", config.no_color));
        }
    }

    Ok(())
}
