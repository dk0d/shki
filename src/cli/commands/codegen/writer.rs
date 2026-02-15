//! File writing utilities for generated code
//!
//! This module provides a trait-based system for writing generated code to files,
//! supporting multiple output languages and file organization modes.

use std::path::{Path, PathBuf};

use crate::{Result, ShkiError};

use super::config::{CodegenConfig, OutputMode};

// ============================================================================
// Writer Trait
// ============================================================================

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
            .output
            .as_ref()
            .ok_or_else(|| ShkiError::Config("No output directory specified".to_string()))?;

        match config.mode {
            OutputMode::SingleFile => self.write_single_file(code, output_dir, config),
            OutputMode::SingleModule => self.write_single_module(code, output_dir, config),
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

    /// Write generated code to a nested module structure
    fn write_modules(
        &self,
        code: &Self::GeneratedCode,
        output_dir: &Path,
        config: &CodegenConfig,
    ) -> Result<Vec<PathBuf>>;

    /// Format generated code as a string for preview/stdout
    fn format_preview(&self, code: &Self::GeneratedCode) -> String;

    /// Get the file extension for this language (without dot)
    fn file_extension(&self) -> &str;

    /// Get the default output filename (without extension)
    fn default_filename(&self) -> &str;
}
