//! Rust code generator
//!
//! Generates Rust structs and enums from database schema snapshots,
//! compatible with sqlx and other database libraries.

use std::collections::HashSet;

use heck::{ToSnakeCase, ToUpperCamelCase};
use indexmap::IndexMap;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::codegen::CodegenConfig;
use crate::codegen::generator::CodeGenerator;
use crate::models::iden::Iden;
use crate::schema::{Column, DataType, DbEnum, Table};
use crate::snapshots::Snapshot;

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
    /// Check if there's any generated code
    pub fn is_empty(&self) -> bool {
        self.enums.is_empty() && self.structs.is_empty()
    }

    /// Get all required imports for the generated code
    pub fn required_imports(&self) -> Vec<String> {
        let mut imports = Vec::new();

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
    type EnumDef = RustEnum;
    type TableDef = RustStruct;

    fn init_output(&self, _config: &CodegenConfig) -> Self::Output {
        GeneratedRust::default()
    }

    fn generate_enum(
        &self,
        name: &Iden,
        enum_snapshot: &DbEnum,
        config: &CodegenConfig,
    ) -> Self::EnumDef {
        self.build_enum(&name.name, enum_snapshot, config)
    }

    fn generate_table(
        &self,
        name: &Iden,
        table_snapshot: &Table,
        snapshot: &Snapshot,
        config: &CodegenConfig,
    ) -> Self::TableDef {
        self.build_struct(name, table_snapshot, &snapshot.enums(), config)
    }

    fn insert_enum(&self, output: &mut Self::Output, name: &Iden, def: Self::EnumDef) {
        output.enums.insert(name.to_string(), def);
    }

    fn insert_table(&self, output: &mut Self::Output, name: &Iden, def: Self::TableDef) {
        output.structs.insert(name.to_string(), def);
    }
}

impl RustGenerator {
    /// Generate a Rust enum from an enum snapshot
    fn build_enum(&self, name: &str, enum_snapshot: &DbEnum, config: &CodegenConfig) -> RustEnum {
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
    fn build_struct(
        &self,
        name: &Iden,
        table: &Table,
        enums: &IndexMap<Iden, DbEnum>,
        config: &CodegenConfig,
    ) -> RustStruct {
        let rust_name = self.transform_struct_name(&name.name, config);

        let fields = table
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
        col: &Column,
        enums: &IndexMap<Iden, DbEnum>,
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

    pub fn sql_type_to_rust(
        &self,
        sql_type: &DataType,
        nullable: bool,
        enums: &IndexMap<Iden, DbEnum>,
        config: &CodegenConfig,
    ) -> String {
        if let Some(override_type) = self.overridden_type(sql_type, config) {
            return wrap_nullable(override_type, nullable);
        }

        let rust_type = match sql_type {
            DataType::Boolean => "bool".to_string(),

            DataType::SmallInt | DataType::SmallSerial => "i16".to_string(),
            DataType::Integer | DataType::Serial | DataType::MediumInt { .. } => "i32".to_string(),
            DataType::BigInt | DataType::BigSerial | DataType::SqliteInteger => "i64".to_string(),
            DataType::TinyInt { .. } => "i8".to_string(),
            DataType::Year => "i16".to_string(),

            DataType::Real | DataType::SqliteReal => "f32".to_string(),
            DataType::DoublePrecision => "f64".to_string(),

            DataType::Numeric { .. } | DataType::Decimal { .. } | DataType::Money => {
                "String".to_string()
            }

            DataType::Char { .. }
            | DataType::VarChar { .. }
            | DataType::Text
            | DataType::Citext
            | DataType::Inet
            | DataType::Cidr
            | DataType::MacAddr
            | DataType::MacAddr8
            | DataType::Interval
            | DataType::Point
            | DataType::Line
            | DataType::LSeg
            | DataType::Box
            | DataType::Path
            | DataType::Polygon
            | DataType::Circle
            | DataType::TinyText
            | DataType::MediumText
            | DataType::LongText
            | DataType::SqliteText
            | DataType::Enum_ { .. }
            | DataType::Set { .. } => "String".to_string(),

            DataType::ByteA
            | DataType::Blob
            | DataType::Binary { .. }
            | DataType::VarBinary { .. }
            | DataType::TinyBlob
            | DataType::MediumBlob
            | DataType::LongBlob
            | DataType::SqliteBlob => "Vec<u8>".to_string(),

            DataType::Uuid => "uuid::Uuid".to_string(),
            DataType::Json | DataType::JsonB => "serde_json::Value".to_string(),
            DataType::Date => "chrono::NaiveDate".to_string(),
            DataType::Time { .. } => "chrono::NaiveTime".to_string(),
            DataType::Timestamp { with_timezone, .. } => {
                if *with_timezone {
                    "chrono::DateTime<chrono::Utc>".to_string()
                } else {
                    "chrono::NaiveDateTime".to_string()
                }
            }

            DataType::Array { element_type } => {
                let inner = self.sql_type_to_rust(element_type, false, enums, config);
                format!("Vec<{}>", inner)
            }

            DataType::Enum { name, schema } => self
                .enum_type_name(name, schema, enums, config)
                .unwrap_or_else(|| "String".to_string()),

            DataType::Custom { name, schema } => self
                .enum_type_name(name, schema, enums, config)
                .unwrap_or_else(|| "String".to_string()),

            DataType::Int4Range
            | DataType::Int8Range
            | DataType::NumRange
            | DataType::TsRange
            | DataType::TsTzRange
            | DataType::DateRange => "String".to_string(),
        };

        wrap_nullable(&rust_type, nullable)
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

impl RustEnum {
    /// Generate the Rust code for this enum as a TokenStream
    pub fn to_tokens(&self) -> TokenStream {
        let name = Ident::new(&self.name, Span::call_site());
        let db_name = &self.db_name;

        let doc = self.comment.as_ref().map(|c| {
            quote! { #[doc = #c] }
        });

        let derives = generate_derives(&self.derives, self.serde);
        let attrs = generate_attributes(&self.attributes);
        let sqlx_type = quote! { #[sqlx(type_name = #db_name)] };
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

    /// Format the enum as a Rust code string
    pub fn to_string_pretty(&self) -> String {
        format_tokens(self.to_tokens())
    }
}

impl RustStruct {
    /// Generate the Rust code for this struct as a TokenStream
    pub fn to_tokens(&self) -> TokenStream {
        let name = Ident::new(&self.name, Span::call_site());

        let doc = self.comment.as_ref().map(|c| {
            quote! { #[doc = #c] }
        });

        let derives = generate_derives(&self.derives, self.serde);
        let attrs = generate_attributes(&self.attributes);
        let fields: Vec<TokenStream> = self
            .fields
            .iter()
            .map(|f| {
                let field_name = rust_field_ident(&f.name);
                let field_type: TokenStream = f.rust_type.parse().expect("Invalid type");

                let mut field_attrs = Vec::new();

                let db_name = &f.db_name;
                if f.name != f.db_name && !f.name.starts_with("r#") {
                    field_attrs.push(quote! { #[sqlx(rename = #db_name)] });
                }
                if f.name.starts_with("r#") {
                    field_attrs.push(quote! { #[sqlx(rename = #db_name)] });
                    if self.serde {
                        field_attrs.push(quote! { #[serde(rename = #db_name)] });
                    }
                }

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

    /// Format the struct as a Rust code string
    pub fn to_string_pretty(&self) -> String {
        format_tokens(self.to_tokens())
    }
}

fn generate_derives(derives: &[String], serde: bool) -> TokenStream {
    if derives.is_empty() && !serde {
        return quote! {};
    }

    let mut derives: HashSet<String> = HashSet::from_iter(derives.iter().cloned());

    if serde {
        derives.insert("serde::Serialize".to_owned());
        derives.insert("serde::Deserialize".to_owned());
    }

    let derives = derives.iter().map(|d| {
        let path: syn::Path = syn::parse_str(d).expect("Invalid derive path");
        quote! { #path }
    });

    quote! { #[derive(#(#derives),*)] }
}

fn generate_attributes(attributes: &[String]) -> Vec<TokenStream> {
    attributes
        .iter()
        .map(|attr| {
            let attr: TokenStream = attr.parse().expect("Invalid attribute");
            quote! { #[#attr] }
        })
        .collect()
}

fn format_tokens(tokens: TokenStream) -> String {
    let file: syn::File = syn::parse2(tokens).expect("Failed to parse generated tokens");
    prettyplease::unparse(&file)
}

fn rust_field_ident(name: &str) -> Ident {
    name.strip_prefix("r#")
        .map(|name| Ident::new_raw(name, Span::call_site()))
        .unwrap_or_else(|| Ident::new(name, Span::call_site()))
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
            generator.sql_type_to_rust(&DataType::Integer, false, &enums, &config),
            "i32"
        );
        assert_eq!(
            generator.sql_type_to_rust(&DataType::Text, true, &enums, &config),
            "Option<String>"
        );
        assert_eq!(
            generator.sql_type_to_rust(
                &DataType::Timestamp {
                    precision: None,
                    with_timezone: true,
                },
                false,
                &enums,
                &config,
            ),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(
            generator.sql_type_to_rust(&DataType::Uuid, false, &enums, &config),
            "uuid::Uuid"
        );
    }

    #[test]
    fn test_sql_type_to_rust_uses_overrides_before_nullability() {
        let enums = IndexMap::new();
        let config = CodegenConfig::default().type_override("jsonb", "MyJson");
        let generator = RustGenerator;

        assert_eq!(
            generator.sql_type_to_rust(&DataType::JsonB, true, &enums, &config),
            "Option<MyJson>"
        );
    }

    #[test]
    fn test_sql_type_to_rust_resolves_enum_and_custom_types() {
        let mut enums = IndexMap::new();
        enums.insert(
            Iden::new("user_status", Some("public".to_string())),
            DbEnum::with_values("user_status", vec!["active", "inactive"]),
        );
        let config = CodegenConfig::default();
        let generator = RustGenerator;

        assert_eq!(
            generator.sql_type_to_rust(
                &DataType::Enum {
                    name: "user_status".to_string(),
                    schema: Some("public".to_string()),
                },
                false,
                &enums,
                &config,
            ),
            "UserStatus"
        );
        assert_eq!(
            generator.sql_type_to_rust(
                &DataType::Custom {
                    name: "unknown_type".to_string(),
                    schema: None,
                },
                false,
                &enums,
                &config,
            ),
            "String"
        );
    }

    #[test]
    fn test_rust_struct_to_tokens_adds_serde_for_raw_identifier() {
        let rust_struct = RustStruct {
            name: "Thing".to_string(),
            table_name: "things".to_string(),
            fields: vec![RustField {
                name: "r#type".to_string(),
                db_name: "type".to_string(),
                rust_type: "String".to_string(),
                nullable: false,
                primary_key: false,
                unique: false,
                comment: None,
            }],
            derives: vec!["Debug".to_string()],
            attributes: vec![],
            serde: true,
            comment: None,
        };

        let output = rust_struct.to_string_pretty();
        assert!(output.contains("serde::Serialize"));
        assert!(output.contains("serde::Deserialize"));
        assert!(output.contains(r#"#[sqlx(rename = "type")]"#));
        assert!(output.contains(r#"#[serde(rename = "type")]"#));
        assert!(output.contains("pub r#type: String"));
    }
}
