//! File writing utilities for generated code
//!
//! This module provides a trait-based system for writing generated code to files,
//! supporting multiple output languages and file organization modes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::display::preview::{PreviewFile, render_preview};
use crate::{Result, ShkiError};

use super::config::{CodegenConfig, OutputMode};

/// Trait for writing generated code to files
///
/// Each language implementation (Rust, Protobuf, etc.) implements this trait
/// to handle language-specific file writing logic.
pub trait CodeWriter {
    /// The type of generated code this writer handles
    type GeneratedCode;

    /// Write generated code to files based on the output mode
    fn write(&self, code: &Self::GeneratedCode, config: &CodegenConfig) -> Result<Vec<PathBuf>> {
        let output_dir = config
            .output()
            .ok_or_else(|| ShkiError::Config("No output directory specified".to_string()))?;

        match config.format() {
            OutputMode::File => self.write_single_file(code, output_dir, config),
            OutputMode::Module => self.write_single_module(code, output_dir, config),
            OutputMode::Modules => self.write_modules(code, output_dir, config),
        }
    }

    /// Write all generated code to a single file
    fn write_single_file(
        &self,
        code: &Self::GeneratedCode,
        output_dir: &Path,
        config: &CodegenConfig,
    ) -> Result<Vec<PathBuf>>;

    /// Write generated code to separate files in a flat module structure
    fn write_single_module(
        &self,
        code: &Self::GeneratedCode,
        output_dir: &Path,
        config: &CodegenConfig,
    ) -> Result<Vec<PathBuf>>;

    /// Default to single module when not specifically implemented
    /// Write generated code to a nested module structure
    fn write_modules(
        &self,
        code: &Self::GeneratedCode,
        output_dir: &Path,
        config: &CodegenConfig,
    ) -> Result<Vec<PathBuf>> {
        println!("Falling back to single-module");
        self.write_single_module(code, output_dir, config)
    }

    /// Raw contents of the single-file output, exactly as written to disk in
    /// [`OutputMode::File`]. Used both for writing and as the File-mode preview.
    fn single_file_contents(&self, code: &Self::GeneratedCode) -> String;

    /// The files this writer would produce for `config.format`, used to build a
    /// preview that reflects the real on-disk layout.
    fn preview_files(&self, code: &Self::GeneratedCode, config: &CodegenConfig)
    -> Vec<PreviewFile>;

    /// The `bat` language token used to syntax-highlight previews.
    fn syntax_language(&self) -> &str;

    /// Format generated code as a string for preview/stdout, highlighting each
    /// file and reflecting whether output is split across separate files.
    fn format_preview(
        &self,
        code: &Self::GeneratedCode,
        config: &CodegenConfig,
        no_color: bool,
    ) -> String {
        render_preview(
            &self.preview_files(code, config),
            self.syntax_language(),
            no_color,
        )
    }

    /// Get the file extension for this language (without dot)
    fn file_extension(&self) -> &str;

    /// Get the default output filename (without extension)
    fn default_filename(&self) -> &str;

    /// Ensure output directory exists
    fn ensure_output_dir(&self, output_dir: &Path) -> Result<()> {
        fs::create_dir_all(output_dir)?;
        Ok(())
    }

    /// Build the default output path for single-file mode
    fn output_file_path(&self, output_dir: &Path) -> PathBuf {
        output_dir.join(format!(
            "{}.{}",
            self.default_filename(),
            self.file_extension()
        ))
    }
}
