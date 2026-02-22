//! Rust code generator
//!
//! Generates Rust structs and enums from database schema snapshots,
//! compatible with sqlx and other database libraries.

use heck::{ToSnakeCase, ToUpperCamelCase};
use indexmap::IndexMap;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::commands::codegen::CodegenConfig;
use crate::commands::codegen::languages::generator::CodeGenerator;
use crate::snapshot::{ColumnSnapshot, EnumSnapshot, Snapshot, TableSnapshot};

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

/// Container for all generated Rust code
#[derive(Debug, Clone, Default)]
pub struct GeneratedRust {
    /// Generated enums (db_name -> RustEnum)
    pub enums: IndexMap<String, RustEnum>,
    /// Generated structs (table_name -> RustStruct)
    pub structs: IndexMap<String, RustStruct>,
}

impl GeneratedRust {
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

/// Rust code generator
#[derive(Default)]
pub struct RustGenerator;

impl RustGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl CodeGenerator for RustGenerator {
    type Output = GeneratedRust;

    /// Generate Rust code from a schema snapshot
    fn generate(&self, snapshot: &Snapshot, config: &CodegenConfig) -> GeneratedRust {
        let mut code = GeneratedRust::new();

        // Generate enums first (structs may depend on them)
        for (name, enum_snapshot) in &snapshot.enums {
            let rust_enum = self.generate_enum(name, enum_snapshot, config);
            code.enums.insert(name.clone(), rust_enum);
        }

        // Generate structs from tables
        for (name, table_snapshot) in &snapshot.tables {
            if !config.should_include_table(name) {
                continue;
            }
            let rust_struct = self.generate_struct(name, table_snapshot, &snapshot.enums, config);
            code.structs.insert(name.clone(), rust_struct);
        }

        code
    }
}

impl RustGenerator {
    /// Generate a Rust enum from an enum snapshot
    fn generate_enum(
        &self,
        name: &str,
        enum_snapshot: &EnumSnapshot,
        config: &CodegenConfig,
    ) -> RustEnum {
        let rust_name = self.transform_enum_name(name, config);

        let variants: Vec<RustEnumVariant> = enum_snapshot
            .values
            .iter()
            .map(|value| {
                let rust_name = value.to_upper_camel_case();
                RustEnumVariant {
                    name: rust_name,
                    db_name: value.clone(),
                }
            })
            .collect();

        RustEnum {
            name: rust_name,
            db_name: name.to_string(),
            variants,
            derives: config.enum_derives.clone(),
            attributes: config.enum_attributes.clone(),
            serde: config.serde,
            comment: enum_snapshot.description.clone(),
        }
    }

    /// Generate a Rust struct from a table snapshot
    fn generate_struct(
        &self,
        name: &str,
        table: &TableSnapshot,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> RustStruct {
        let rust_name = self.transform_struct_name(name, config);

        let fields: Vec<RustField> = table
            .columns
            .values()
            .map(|col| self.generate_field(col, enums, config))
            .collect();

        RustStruct {
            name: rust_name,
            table_name: name.to_string(),
            fields,
            derives: config.struct_derives.clone(),
            attributes: config.struct_attributes.clone(),
            serde: config.serde,
            comment: table.comment.clone(),
        }
    }

    /// Generate a Rust field from a column snapshot
    fn generate_field(
        &self,
        col: &ColumnSnapshot,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> RustField {
        let field_name = make_safe_field_name(&col.name);
        let rust_type = self.sql_type_to_rust(&col.data_type, col.nullable, enums, config);

        RustField {
            name: field_name,
            db_name: col.name.clone(),
            rust_type,
            nullable: col.nullable,
            primary_key: col.primary_key,
            unique: col.unique,
            comment: col.comment.clone(),
        }
    }

    /// Convert a SQL type string to a Rust type string
    pub fn sql_type_to_rust(
        &self,
        sql_type: &str,
        nullable: bool,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> String {
        // Check for type overrides first
        let normalized = sql_type.to_lowercase();
        if let Some(override_type) = config.type_overrides.get(&normalized) {
            return wrap_nullable(override_type, nullable);
        }

        // Strip quotes from the type name for enum lookup
        let unquoted = sql_type
            .trim_matches('"')
            .split('.')
            .next_back()
            .unwrap_or(sql_type)
            .trim_matches('"');

        // Check if it's a known enum type (by original name or unquoted name)
        if enums.contains_key(sql_type) {
            let enum_name = sql_type.to_upper_camel_case();
            return wrap_nullable(&enum_name, nullable);
        }
        if enums.contains_key(unquoted) {
            let enum_name = unquoted.to_upper_camel_case();
            return wrap_nullable(&enum_name, nullable);
        }

        // Handle array types
        if let Some(inner) = sql_type.strip_suffix("[]") {
            let inner_rust = self.sql_type_to_rust(inner, false, enums, config);
            return format!("Vec<{}>", inner_rust);
        }

        // Map SQL types to Rust types
        let rust_type = match normalized.as_str() {
            // Boolean
            "bool" | "boolean" => "bool",

            // Integers
            "smallint" | "int2" | "smallserial" => "i16",
            "integer" | "int" | "int4" | "serial" => "i32",
            "bigint" | "int8" | "bigserial" => "i64",

            // Floating point
            "real" | "float4" => "f32",
            "double precision" | "float8" => "f64",

            // Numeric/Decimal - default to String, can be overridden
            "numeric" | "decimal" => "String",

            // Text
            "text" | "varchar" | "char" | "character varying" | "character" | "citext" | "name" => {
                "String"
            }

            // Binary
            "bytea" | "blob" => "Vec<u8>",

            // UUID
            "uuid" => "uuid::Uuid",

            // JSON
            "json" | "jsonb" => "serde_json::Value",

            // Date/Time
            "date" => "chrono::NaiveDate",
            "time" | "time without time zone" => "chrono::NaiveTime",
            "timestamp" | "timestamp without time zone" => "chrono::NaiveDateTime",
            "timestamp with time zone" | "timestamptz" => "chrono::DateTime<chrono::Utc>",

            // Network types
            "inet" | "cidr" => "String", // Could be ipnetwork::IpNetwork with feature
            "macaddr" | "macaddr8" => "String",

            // PostgreSQL specific
            "money" => "String",
            "interval" => "String",
            "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => "String",
            "tsquery" | "tsvector" => "String",
            "xml" => "String",

            // MySQL specific
            "tinyint" => "i8",
            "mediumint" => "i32",
            "year" => "i16",
            "datetime" => "chrono::NaiveDateTime",
            "enum" => "String", // MySQL inline enums

            // SQLite specific
            "integer primary key" => "i64", // SQLite rowid alias

            // Default to String for unknown types
            _ => {
                // Check for varchar(n), char(n), etc.
                if normalized.starts_with("varchar")
                    || normalized.starts_with("character varying")
                    || normalized.starts_with("char")
                    || normalized.starts_with("character")
                    || normalized.starts_with("numeric")
                    || normalized.starts_with("decimal")
                {
                    "String"
                } else if normalized.starts_with("timestamp") {
                    if normalized.contains("with time zone") {
                        "chrono::DateTime<chrono::Utc>"
                    } else {
                        "chrono::NaiveDateTime"
                    }
                } else if normalized.starts_with("time") {
                    "chrono::NaiveTime"
                } else {
                    "String" // Fallback
                }
            }
        };

        wrap_nullable(rust_type, nullable)
    }
}

/// Wrap a type in Option if nullable
fn wrap_nullable(rust_type: &str, nullable: bool) -> String {
    if nullable {
        format!("Option<{}>", rust_type)
    } else {
        rust_type.to_string()
    }
}

/// Make a field name safe for Rust (handle reserved keywords)
pub fn make_safe_field_name(name: &str) -> String {
    let snake = name.to_snake_case();

    // Rust reserved keywords that might be used as column names
    const RESERVED: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self",
        // `Self` to snake case gives `self` which means this should never match, but kept for
        // completeness, and in case snake case logic changes in future.
        "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
        "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
        "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];

    if RESERVED.contains(&snake.as_str()) {
        format!("r#{}", snake)
    } else {
        snake
    }
}

// ============================================================================
// Token generation implementations for RustEnum and RustStruct
// ============================================================================

impl RustEnum {
    /// Generate the Rust code for this enum as a TokenStream
    pub fn to_tokens(&self) -> TokenStream {
        let name = Ident::new(&self.name, Span::call_site());
        let db_name = &self.db_name;

        // Generate doc comment
        let doc = self.comment.as_ref().map(|c| {
            quote! { #[doc = #c] }
        });

        // Generate derives
        let derives = self.generate_derives();

        // Generate additional attributes
        let attrs = self.generate_attributes();

        // Generate sqlx type attribute
        let sqlx_type = quote! { #[sqlx(type_name = #db_name)] };

        // Generate variants
        let variants: Vec<TokenStream> = self
            .variants
            .iter()
            .map(|v| {
                let variant_name = Ident::new(&v.name, Span::call_site());
                let variant_db_name = &v.db_name;

                let mut variant_attrs = vec![quote! { #[sqlx(rename = #variant_db_name)] }];

                if self.serde && v.name != v.db_name {
                    variant_attrs.push(quote! { #[serde(rename = #variant_db_name)] });
                }

                quote! {
                    #(#variant_attrs)*
                    #variant_name
                }
            })
            .collect();

        quote! {
            #doc
            #derives
            #(#attrs)*
            #sqlx_type
            pub enum #name {
                #(#variants),*
            }
        }
    }

    fn generate_derives(&self) -> TokenStream {
        if self.derives.is_empty() {
            return quote! {};
        }

        let derives: Vec<TokenStream> = self
            .derives
            .iter()
            .map(|d| {
                let path: syn::Path = syn::parse_str(d).expect("Invalid derive path");
                quote! { #path }
            })
            .collect();

        quote! { #[derive(#(#derives),*)] }
    }

    fn generate_attributes(&self) -> Vec<TokenStream> {
        self.attributes
            .iter()
            .map(|attr| {
                let attr: TokenStream = attr.parse().expect("Invalid attribute");
                quote! { #[#attr] }
            })
            .collect()
    }

    /// Format the enum as a Rust code string
    pub fn to_string_pretty(&self) -> String {
        let tokens = self.to_tokens();
        let file: syn::File = syn::parse2(tokens).expect("Failed to parse generated tokens");
        prettyplease::unparse(&file)
    }
}

impl RustStruct {
    /// Generate the Rust code for this struct as a TokenStream
    pub fn to_tokens(&self) -> TokenStream {
        let name = Ident::new(&self.name, Span::call_site());

        // Generate doc comment
        let doc = self.comment.as_ref().map(|c| {
            quote! { #[doc = #c] }
        });

        // Generate derives
        let derives = self.generate_derives();

        // Generate additional attributes
        let attrs = self.generate_attributes();

        // Generate fields
        let fields: Vec<TokenStream> = self
            .fields
            .iter()
            .map(|f| {
                let field_name = Ident::new(&f.name, Span::call_site());
                let field_type: TokenStream = f.rust_type.parse().expect("Invalid type");

                let mut field_attrs = Vec::new();

                // Add rename attribute if field name differs from db name
                let db_name = &f.db_name;
                if f.name != f.db_name && !f.name.starts_with("r#") {
                    field_attrs.push(quote! { #[sqlx(rename = #db_name)] });
                }
                // Handle r# prefix - need to rename to original db name
                if f.name.starts_with("r#") {
                    field_attrs.push(quote! { #[sqlx(rename = #db_name)] });
                    if self.serde {
                        field_attrs.push(quote! { #[serde(rename = #db_name)] });
                    }
                }

                // Add doc comment
                let doc = f.comment.as_ref().map(|c| {
                    quote! { #[doc = #c] }
                });

                quote! {
                    #doc
                    #(#field_attrs)*
                    pub #field_name: #field_type
                }
            })
            .collect();

        quote! {
            #doc
            #derives
            #(#attrs)*
            pub struct #name {
                #(#fields),*
            }
        }
    }

    fn generate_derives(&self) -> TokenStream {
        if self.derives.is_empty() {
            return quote! {};
        }

        let derives: Vec<TokenStream> = self
            .derives
            .iter()
            .map(|d| {
                let path: syn::Path = syn::parse_str(d).expect("Invalid derive path");
                quote! { #path }
            })
            .collect();

        quote! { #[derive(#(#derives),*)] }
    }

    fn generate_attributes(&self) -> Vec<TokenStream> {
        self.attributes
            .iter()
            .map(|attr| {
                let attr: TokenStream = attr.parse().expect("Invalid attribute");
                quote! { #[#attr] }
            })
            .collect()
    }

    /// Format the struct as a Rust code string
    pub fn to_string_pretty(&self) -> String {
        let tokens = self.to_tokens();
        let file: syn::File = syn::parse2(tokens).expect("Failed to parse generated tokens");
        prettyplease::unparse(&file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_safe_field_name() {
        let test_cases = vec![
            ("as", "r#as"),
            ("break", "r#break"),
            ("const", "r#const"),
            ("continue", "r#continue"),
            ("crate", "r#crate"),
            ("user_id", "user_id"),
            ("name", "name"),
        ];

        for (input, expected) in test_cases {
            assert_eq!(make_safe_field_name(input), expected);
        }
    }

    #[test]
    fn test_sql_type_to_rust() {
        let enums = IndexMap::new();
        let config = CodegenConfig::default();

        let generator = RustGenerator;

        assert_eq!(
            generator.sql_type_to_rust("INTEGER", false, &enums, &config),
            "i32"
        );
        assert_eq!(
            generator.sql_type_to_rust("TEXT", true, &enums, &config),
            "Option<String>"
        );
        assert_eq!(
            generator.sql_type_to_rust("TIMESTAMP WITH TIME ZONE", false, &enums, &config),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(
            generator.sql_type_to_rust("UUID", false, &enums, &config),
            "uuid::Uuid"
        );
    }
}
