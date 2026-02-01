//! Generated Rust type representations

use indexmap::IndexMap;

/// Container for all generated Rust code
#[derive(Debug, Clone, Default)]
pub struct GeneratedCode {
    /// Generated enums (db_name -> RustEnum)
    pub enums: IndexMap<String, RustEnum>,
    /// Generated structs (table_name -> RustStruct)
    pub structs: IndexMap<String, RustStruct>,
}

impl GeneratedCode {
    /// Create a new empty GeneratedCode
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there's any generated code
    pub fn is_empty(&self) -> bool {
        self.enums.is_empty() && self.structs.is_empty()
    }

    /// Get all required imports for the generated code
    pub fn required_imports(&self) -> Vec<String> {
        let mut imports = Vec::new();

        // Collect types from all fields
        for rust_struct in self.structs.values() {
            for field in &rust_struct.fields {
                extract_imports(&field.rust_type, &mut imports);
            }
        }

        imports.sort();
        imports.dedup();
        imports
    }
}

/// Extract import requirements from a type string
fn extract_imports(rust_type: &str, imports: &mut Vec<String>) {
    if rust_type.contains("chrono::") {
        imports.push("use chrono;".to_string());
    }
    if rust_type.contains("uuid::") {
        imports.push("use uuid;".to_string());
    }
    if rust_type.contains("serde_json::") {
        imports.push("use serde_json;".to_string());
    }
}

/// A generated Rust enum
#[derive(Debug, Clone)]
pub struct RustEnum {
    /// Rust enum name (PascalCase)
    pub name: String,
    /// Original database enum name
    pub db_name: String,
    /// Enum variants
    pub variants: Vec<RustEnumVariant>,
    /// Derive macros
    pub derives: Vec<String>,
    /// Additional attributes
    pub attributes: Vec<String>,
    /// Whether serde support is enabled
    pub serde: bool,
    /// Doc comment
    pub comment: Option<String>,
}

/// A variant of a Rust enum
#[derive(Debug, Clone)]
pub struct RustEnumVariant {
    /// Rust variant name (PascalCase)
    pub name: String,
    /// Original database value
    pub db_name: String,
}

/// A generated Rust struct
#[derive(Debug, Clone)]
pub struct RustStruct {
    /// Rust struct name (PascalCase, singular)
    pub name: String,
    /// Original table name
    pub table_name: String,
    /// Struct fields
    pub fields: Vec<RustField>,
    /// Derive macros
    pub derives: Vec<String>,
    /// Additional attributes
    pub attributes: Vec<String>,
    /// Whether serde support is enabled
    pub serde: bool,
    /// Doc comment
    pub comment: Option<String>,
}

/// A field in a Rust struct
#[derive(Debug, Clone)]
pub struct RustField {
    /// Rust field name (snake_case, may have r# prefix)
    pub name: String,
    /// Original column name
    pub db_name: String,
    /// Rust type (e.g., "i32", "Option<String>")
    pub rust_type: String,
    /// Whether the field is nullable
    pub nullable: bool,
    /// Whether this is a primary key
    pub primary_key: bool,
    /// Whether this has a unique constraint
    pub unique: bool,
    /// Doc comment
    pub comment: Option<String>,
}
