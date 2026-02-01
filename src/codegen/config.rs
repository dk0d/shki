//! Code generation configuration

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for Rust code generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenConfig {
    /// Output directory for generated code
    #[serde(default)]
    pub output_dir: Option<PathBuf>,

    /// Output mode: single file or module directory
    #[serde(default)]
    pub output_mode: OutputMode,

    /// Derives to add to generated structs
    #[serde(default = "default_struct_derives")]
    pub struct_derives: Vec<String>,

    /// Additional attributes for generated structs
    #[serde(default)]
    pub struct_attributes: Vec<String>,

    /// Derives to add to generated enums
    #[serde(default = "default_enum_derives")]
    pub enum_derives: Vec<String>,

    /// Additional attributes for generated enums
    #[serde(default)]
    pub enum_attributes: Vec<String>,

    /// Custom struct name overrides (table_name -> RustStructName)
    #[serde(default)]
    pub struct_renames: IndexMap<String, String>,

    /// Custom enum name overrides (enum_name -> RustEnumName)
    #[serde(default)]
    pub enum_renames: IndexMap<String, String>,

    /// SQL type to Rust type overrides
    #[serde(default)]
    pub type_overrides: IndexMap<String, String>,

    /// Whether to add serde derives and rename attributes
    #[serde(default)]
    pub serde: bool,

    /// Whether to generate sqlx::FromRow derive
    #[serde(default = "default_true")]
    pub sqlx: bool,

    /// Tables to include (empty = all)
    #[serde(default)]
    pub include_tables: Vec<String>,

    /// Tables to exclude
    #[serde(default)]
    pub exclude_tables: Vec<String>,

    /// Whether to generate a mod.rs file (for module output mode)
    #[serde(default = "default_true")]
    pub generate_mod_file: bool,
}

/// Output mode for generated code
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Generate a single file with all structs and enums
    #[default]
    SingleFile,
    /// Generate a module directory with one file per struct/enum
    Module,
}

fn default_struct_derives() -> Vec<String> {
    vec![
        "Debug".to_string(),
        "Clone".to_string(),
        "sqlx::FromRow".to_string(),
    ]
}

fn default_enum_derives() -> Vec<String> {
    vec![
        "Debug".to_string(),
        "Clone".to_string(),
        "PartialEq".to_string(),
        "sqlx::Type".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            output_dir: None,
            output_mode: OutputMode::SingleFile,
            struct_derives: default_struct_derives(),
            struct_attributes: Vec::new(),
            enum_derives: default_enum_derives(),
            enum_attributes: Vec::new(),
            struct_renames: IndexMap::new(),
            enum_renames: IndexMap::new(),
            type_overrides: IndexMap::new(),
            serde: false,
            sqlx: true,
            include_tables: Vec::new(),
            exclude_tables: Vec::new(),
            generate_mod_file: true,
        }
    }
}

impl CodegenConfig {
    /// Create a new default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the output directory
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    /// Set single file output mode
    pub fn single_file(mut self) -> Self {
        self.output_mode = OutputMode::SingleFile;
        self
    }

    /// Set module output mode
    pub fn module(mut self) -> Self {
        self.output_mode = OutputMode::Module;
        self
    }

    /// Add a derive to structs
    pub fn struct_derive(mut self, derive: impl Into<String>) -> Self {
        self.struct_derives.push(derive.into());
        self
    }

    /// Set struct derives (replaces defaults)
    pub fn struct_derives(mut self, derives: Vec<impl Into<String>>) -> Self {
        self.struct_derives = derives.into_iter().map(Into::into).collect();
        self
    }

    /// Add a derive to enums
    pub fn enum_derive(mut self, derive: impl Into<String>) -> Self {
        self.enum_derives.push(derive.into());
        self
    }

    /// Set enum derives (replaces defaults)
    pub fn enum_derives(mut self, derives: Vec<impl Into<String>>) -> Self {
        self.enum_derives = derives.into_iter().map(Into::into).collect();
        self
    }

    /// Add a struct attribute
    pub fn struct_attribute(mut self, attr: impl Into<String>) -> Self {
        self.struct_attributes.push(attr.into());
        self
    }

    /// Add an enum attribute
    pub fn enum_attribute(mut self, attr: impl Into<String>) -> Self {
        self.enum_attributes.push(attr.into());
        self
    }

    /// Rename a struct (table_name -> rust_name)
    pub fn rename_struct(
        mut self,
        table_name: impl Into<String>,
        rust_name: impl Into<String>,
    ) -> Self {
        self.struct_renames
            .insert(table_name.into(), rust_name.into());
        self
    }

    /// Rename an enum
    pub fn rename_enum(
        mut self,
        enum_name: impl Into<String>,
        rust_name: impl Into<String>,
    ) -> Self {
        self.enum_renames.insert(enum_name.into(), rust_name.into());
        self
    }

    /// Override a SQL type mapping
    pub fn type_override(
        mut self,
        sql_type: impl Into<String>,
        rust_type: impl Into<String>,
    ) -> Self {
        self.type_overrides
            .insert(sql_type.into().to_lowercase(), rust_type.into());
        self
    }

    /// Enable serde support
    pub fn with_serde(mut self) -> Self {
        self.serde = true;
        if !self
            .struct_derives
            .contains(&"serde::Serialize".to_string())
        {
            self.struct_derives.push("serde::Serialize".to_string());
            self.struct_derives.push("serde::Deserialize".to_string());
        }
        if !self.enum_derives.contains(&"serde::Serialize".to_string()) {
            self.enum_derives.push("serde::Serialize".to_string());
            self.enum_derives.push("serde::Deserialize".to_string());
        }
        self
    }

    /// Disable sqlx derives
    pub fn without_sqlx(mut self) -> Self {
        self.sqlx = false;
        self.struct_derives.retain(|d| !d.contains("sqlx"));
        self.enum_derives.retain(|d| !d.contains("sqlx"));
        self
    }

    /// Include only specific tables
    pub fn include_tables(mut self, tables: Vec<impl Into<String>>) -> Self {
        self.include_tables = tables.into_iter().map(Into::into).collect();
        self
    }

    /// Exclude specific tables
    pub fn exclude_tables(mut self, tables: Vec<impl Into<String>>) -> Self {
        self.exclude_tables = tables.into_iter().map(Into::into).collect();
        self
    }

    /// Check if a table should be included
    pub fn should_include_table(&self, table_name: &str) -> bool {
        // If include list is specified, table must be in it
        if !self.include_tables.is_empty() && !self.include_tables.contains(&table_name.to_string())
        {
            return false;
        }
        // If in exclude list, skip
        if self.exclude_tables.contains(&table_name.to_string()) {
            return false;
        }
        true
    }
}
