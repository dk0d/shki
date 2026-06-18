//! Typed query codegen.
//!
//! Reads annotated `*.sql` files, describes each query against the compiled
//! Shadow Database to resolve parameter and result types, and generates
//! type-safe Rust functions that wrap them with sqlx. See
//! `docs/adr/0001-typed-query-codegen.md`.

pub mod config;
pub mod describe;
pub mod generator;
pub mod parse;
pub mod rewrite;

pub use config::QueriesConfig;

use std::path::Path;

use crate::compiler::with_compiled_shadow;
use crate::config::Config;
use crate::display::preview::{PreviewFile, render_preview};
use crate::utils::resolve_path;
use crate::{Result, ShkiError};

use describe::describe_query;
use generator::generate_rust_module;
use parse::parse_query_dir;

/// Generate typed query wrappers from a directory of annotated SQL files.
///
/// `preview` is a CLI-only flag (not part of the merged `[queries]` config), so
/// it is passed explicitly rather than read from `config`.
pub async fn cmd_query_codegen(config: &Config, preview: bool) -> Result<()> {
    let dir = config
        .queries
        .sources
        .as_ref()
        .map(|d| resolve_path(Some(config.root.clone()), d))
        .unwrap_or_else(|| config.root.join("queries"));

    let specs = parse_query_dir(&dir)?;
    if specs.is_empty() {
        println!("No queries found in {}", dir.display());
        return Ok(());
    }
    let query_count = specs.len();

    // The Rust path the generated module imports schema types from, e.g.
    // `use super::models::*;`. Validated/derived up front so we never emit a
    // malformed `use`.
    let models_module = resolve_models_module(config)?;

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
    let preview_filename = config
        .queries
        .output
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "queries.rs".to_string());

    match config.queries.output.as_ref() {
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
            println!("{}", render_preview(&[file], "rust", config.no_color()));
        }
    }

    Ok(())
}

/// Resolve the Rust module path the generated query module imports schema types
/// from (`use <path>::*;`).
///
/// An explicit `[queries] models` wins; otherwise the path is derived
/// from the relationship between the schema codegen output and the queries
/// output (see [`derive_models_module`]). Either way the result is validated as
/// a real Rust `use` path so a stray file path can never reach the generator.
fn resolve_models_module(config: &Config) -> Result<Option<String>> {
    let candidate = match &config.queries.models {
        Some(explicit) => Some(explicit.clone()),
        None => derive_models_module(config),
    };

    if let Some(path) = &candidate {
        syn::parse_str::<syn::ItemUse>(&format!("use {}::*;", path)).map_err(|_| {
            ShkiError::config(format!(
                "[queries] models must be a Rust module path like `crate::models` or \
                 `super::models`, not a file path: `{}`",
                path
            ))
        })?;
    }

    Ok(candidate)
}

/// Derive the import path to the schema codegen output from the queries output,
/// e.g. sibling files `src/db/{models.rs, queries.rs}` -> `super::models`.
///
/// A Rust module's `super` is its containing directory, so the path is computed
/// purely from the directory relationship between the two outputs — no
/// knowledge of the crate root is required. The module name is the codegen
/// output's file stem, which is the module in both single-file (`models.rs`)
/// and module-directory (`models/`) layouts. Returns `None` when either output
/// path is unset (e.g. stdout preview), in which case no import is emitted.
fn derive_models_module(config: &Config) -> Option<String> {
    let models_out = config.codegen.output()?;
    let queries_out = config.queries.output.as_ref()?;

    let models_name = models_out.file_stem()?.to_str()?.to_string();
    let queries_dir = queries_out.parent().unwrap_or_else(|| Path::new(""));
    let models_dir = models_out.parent().unwrap_or_else(|| Path::new(""));

    let (up, down) = module_route(queries_dir, models_dir);

    // One `super` climbs from the queries *file* module to its directory module;
    // `up` more climb to the common ancestor, then `down` descends to the models
    // directory, then the models module name itself.
    let mut segments: Vec<String> = vec!["super".to_string(); up + 1];
    segments.extend(down);
    segments.push(models_name);
    Some(segments.join("::"))
}

/// The directory hop from `from` to `to` as a number of parent steps (`..`) and
/// the forward path components after the common prefix.
fn module_route(from: &Path, to: &Path) -> (usize, Vec<String>) {
    let segments = |p: &Path| -> Vec<String> {
        p.components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect()
    };
    let from = segments(from);
    let to = segments(to);

    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    (from.len() - common, to[common..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config_with_outputs(models: &str, queries: &str) -> Config {
        let mut config = Config::default();
        config.codegen.output = Some(PathBuf::from(models));
        config.queries.output = Some(PathBuf::from(queries));
        config
    }

    #[test]
    fn derives_super_path_for_sibling_files() {
        let config = config_with_outputs("src/db/models.rs", "src/db/queries.rs");
        assert_eq!(
            resolve_models_module(&config).unwrap().as_deref(),
            Some("super::models")
        );
    }

    #[test]
    fn derives_super_path_for_module_directory_output() {
        // `modules`/`singlemodule` codegen writes a directory; its name is the module.
        let config = config_with_outputs("src/db/models", "src/db/queries.rs");
        assert_eq!(
            resolve_models_module(&config).unwrap().as_deref(),
            Some("super::models")
        );
    }

    #[test]
    fn derives_path_when_models_live_one_level_up() {
        let config = config_with_outputs("src/models.rs", "src/db/queries.rs");
        assert_eq!(
            resolve_models_module(&config).unwrap().as_deref(),
            Some("super::super::models")
        );
    }

    #[test]
    fn derives_path_when_models_nested_below_queries() {
        let config = config_with_outputs("src/db/models.rs", "src/queries.rs");
        assert_eq!(
            resolve_models_module(&config).unwrap().as_deref(),
            Some("super::db::models")
        );
    }

    #[test]
    fn explicit_module_path_overrides_derivation() {
        let mut config = config_with_outputs("src/db/models.rs", "src/db/queries.rs");
        config.queries.models = Some("crate::models".to_string());
        assert_eq!(
            resolve_models_module(&config).unwrap().as_deref(),
            Some("crate::models")
        );
    }

    #[test]
    fn explicit_file_path_is_rejected() {
        let mut config = Config::default();
        config.queries.models = Some("crates/db/models.rs".to_string());
        let err = resolve_models_module(&config).unwrap_err();
        assert!(err.to_string().contains("Rust module path"), "got: {err}");
    }

    #[test]
    fn no_import_when_output_paths_are_unset() {
        // Preview/stdout: no queries output file to anchor a relative path.
        assert_eq!(resolve_models_module(&Config::default()).unwrap(), None);
    }
}
