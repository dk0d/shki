//! TypeScript code generator
//!
//! Generates TypeScript interfaces and enums from database schema snapshots.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use indexmap::IndexMap;

use crate::commands::codegen::CodegenConfig;
use crate::commands::codegen::languages::generator::CodeGenerator;
use crate::snapshot::{ColumnSnapshot, EnumSnapshot, Snapshot, TableSnapshot};
/// Generated TypeScript code
#[derive(Debug, Clone, Default)]
pub struct GeneratedTypeScript {
    /// Generated enum definitions
    pub enums: IndexMap<String, TypeScriptEnum>,
    /// Generated interface definitions
    pub interfaces: IndexMap<String, TypeScriptInterface>,
}

/// A generated TypeScript enum
#[derive(Debug, Clone)]
pub struct TypeScriptEnum {
    /// TypeScript enum name (PascalCase)
    pub name: String,
    /// Original database enum name
    pub db_name: String,
    /// Enum members
    pub members: Vec<TypeScriptEnumMember>,
    /// Doc comment
    pub comment: Option<String>,
}

/// A member of a TypeScript enum
#[derive(Debug, Clone)]
pub struct TypeScriptEnumMember {
    /// TypeScript member name (PascalCase)
    pub name: String,
    /// Original database value (used as string value)
    pub db_value: String,
}

/// A generated TypeScript interface
#[derive(Debug, Clone)]
pub struct TypeScriptInterface {
    /// TypeScript interface name (PascalCase, singular)
    pub name: String,
    /// Original table name
    pub table_name: String,
    /// Interface properties
    pub properties: Vec<TypeScriptProperty>,
    /// Doc comment
    pub comment: Option<String>,
    /// Export as interface or type
    pub flavor: TypescriptFlavor,
}

/// A property in a TypeScript interface
#[derive(Debug, Clone)]
pub struct TypeScriptProperty {
    /// TypeScript property name (camelCase)
    pub name: String,
    /// Original column name
    pub db_name: String,
    /// TypeScript type (e.g., "string", "number", "Date")
    pub ts_type: String,
    /// Whether the property is nullable/optional
    pub nullable: bool,
    /// Doc comment
    pub comment: Option<String>,
}

impl GeneratedTypeScript {
    /// Create a new empty GeneratedTypeScript
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there's any generated code
    pub fn is_empty(&self) -> bool {
        self.enums.is_empty() && self.interfaces.is_empty()
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum TypescriptFlavor {
    Type,
    #[default]
    Interface,
}

/// TypeScript code generator
#[derive(Default)]
pub struct TypeScriptGenerator {
    pub flavor: TypescriptFlavor,
}

impl TypeScriptGenerator {
    pub fn new(flavor: TypescriptFlavor) -> Self {
        Self { flavor }
    }
}

impl CodeGenerator for TypeScriptGenerator {
    type Output = GeneratedTypeScript;

    /// Generate TypeScript code from a schema snapshot
    fn generate(&self, snapshot: &Snapshot, config: &CodegenConfig) -> GeneratedTypeScript {
        let mut code = GeneratedTypeScript::new();

        // Generate enums first (interfaces may depend on them)
        for (name, enum_snapshot) in &snapshot.enums {
            let ts_enum = self.generate_enum(name, enum_snapshot, config);
            code.enums.insert(name.clone(), ts_enum);
        }

        // Generate interfaces from tables
        for (name, table_snapshot) in &snapshot.tables {
            if !config.should_include_table(name) {
                continue;
            }
            let ts_interface =
                self.generate_interface(name, table_snapshot, &snapshot.enums, config);
            code.interfaces.insert(name.clone(), ts_interface);
        }

        code
    }
}

impl TypeScriptGenerator {
    /// Generate a TypeScript enum from an enum snapshot
    fn generate_enum(
        &self,
        name: &str,
        enum_snapshot: &EnumSnapshot,
        config: &CodegenConfig,
    ) -> TypeScriptEnum {
        let ts_name = self.transform_enum_name(name, config);

        let members: Vec<TypeScriptEnumMember> = enum_snapshot
            .values
            .iter()
            .map(|value| TypeScriptEnumMember {
                name: value.to_upper_camel_case(),
                db_value: value.clone(),
            })
            .collect();

        TypeScriptEnum {
            name: ts_name,
            db_name: name.to_string(),
            members,
            comment: enum_snapshot.description.clone(),
        }
    }

    /// Generate a TypeScript interface from a table snapshot
    fn generate_interface(
        &self,
        name: &str,
        table: &TableSnapshot,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> TypeScriptInterface {
        let ts_name = self.transform_struct_name(name, config);

        let properties: Vec<TypeScriptProperty> = table
            .columns
            .values()
            .map(|col| self.generate_property(col, enums, config))
            .collect();

        TypeScriptInterface {
            name: ts_name,
            table_name: name.to_string(),
            properties,
            comment: table.comment.clone(),
            flavor: self.flavor,
        }
    }

    /// Generate a TypeScript property from a column snapshot
    fn generate_property(
        &self,
        col: &ColumnSnapshot,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> TypeScriptProperty {
        let property_name = col.name.to_lower_camel_case();
        let ts_type = self.sql_type_to_typescript(&col.data_type, enums, config);

        TypeScriptProperty {
            name: property_name,
            db_name: col.name.clone(),
            ts_type,
            nullable: col.nullable,
            comment: col.comment.clone(),
        }
    }

    /// Convert a SQL type to a TypeScript type
    pub fn sql_type_to_typescript(
        &self,
        sql_type: &str,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> String {
        // Check for type overrides first
        let normalized = sql_type.to_lowercase();
        if let Some(override_type) = config.type_overrides.get(&normalized) {
            return override_type.clone();
        }

        // Strip quotes from the type name for enum lookup
        let unquoted = sql_type
            .trim_matches('"')
            .split('.')
            .next_back()
            .unwrap_or(sql_type)
            .trim_matches('"');

        // Check if it's a known enum type
        if enums.contains_key(sql_type) {
            return sql_type.to_upper_camel_case();
        }
        if enums.contains_key(unquoted) {
            return unquoted.to_upper_camel_case();
        }

        // Handle array types
        if let Some(inner) = sql_type.strip_suffix("[]") {
            let inner_ts = self.sql_type_to_typescript(inner, enums, config);
            return format!("{}[]", inner_ts);
        }

        // Map SQL types to TypeScript types
        match normalized.as_str() {
            // Boolean
            "bool" | "boolean" => "boolean".to_string(),

            // Integers and floating point -> number
            "smallint" | "int2" | "smallserial" | "integer" | "int" | "int4" | "serial"
            | "bigint" | "int8" | "bigserial" | "real" | "float4" | "double precision"
            | "float8" | "tinyint" | "mediumint" | "year" => "number".to_string(),

            // Numeric/Decimal - use string to preserve precision, or number
            "numeric" | "decimal" | "money" => "string".to_string(),

            // Text types
            "text" | "varchar" | "char" | "character varying" | "character" | "citext" | "name"
            | "xml" => "string".to_string(),

            // Binary -> Uint8Array or Buffer
            "bytea" | "blob" => "Uint8Array".to_string(),

            // UUID -> string
            "uuid" => "string".to_string(),

            // JSON -> unknown or any (can be overridden)
            "json" | "jsonb" => "unknown".to_string(),

            // Date/Time types
            "date" | "time" | "time without time zone" | "time with time zone" => {
                "string".to_string()
            }
            "timestamp"
            | "timestamp without time zone"
            | "timestamp with time zone"
            | "timestamptz"
            | "datetime" => "Date".to_string(),

            // Interval -> string
            "interval" => "string".to_string(),

            // Network types -> string
            "inet" | "cidr" | "macaddr" | "macaddr8" => "string".to_string(),

            // Geometric types -> string
            "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => {
                "string".to_string()
            }

            // Text search types -> string
            "tsquery" | "tsvector" => "string".to_string(),

            // MySQL inline enums -> string
            "enum" => "string".to_string(),

            // Default handling
            _ => {
                // Check for varchar(n), char(n), etc.
                if normalized.starts_with("varchar")
                    || normalized.starts_with("character varying")
                    || normalized.starts_with("char")
                    || normalized.starts_with("character")
                    || normalized.starts_with("numeric")
                    || normalized.starts_with("decimal")
                {
                    "string".to_string()
                } else if normalized.starts_with("timestamp") {
                    "Date".to_string()
                } else if normalized.starts_with("time") {
                    "string".to_string()
                } else {
                    // Fallback to unknown for unknown types
                    "unknown".to_string()
                }
            }
        }
    }
}

// ============================================================================
// Display implementations for TypeScript code generation
// ============================================================================

impl std::fmt::Display for GeneratedTypeScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Write enums
        for ts_enum in self.enums.values() {
            writeln!(f, "{}", ts_enum)?;
        }

        // Write interfaces
        for ts_interface in self.interfaces.values() {
            writeln!(f, "{}", ts_interface)?;
        }

        Ok(())
    }
}

impl std::fmt::Display for TypeScriptEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Doc comment
        if let Some(comment) = &self.comment {
            writeln!(f, "/** {} */", comment)?;
        }

        writeln!(f, "export enum {} {{", self.name)?;

        for member in &self.members {
            writeln!(f, "  {} = \"{}\",", member.name, member.db_value)?;
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl std::fmt::Display for TypeScriptInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Doc comment
        if let Some(comment) = &self.comment {
            writeln!(f, "/** {} */", comment)?;
        }

        // Table name annotation
        writeln!(f, "/** Table: {} */", self.table_name)?;
        match self.flavor {
            TypescriptFlavor::Interface => writeln!(f, "export interface {} {{", self.name)?,
            TypescriptFlavor::Type => writeln!(f, "export type {} = {{", self.name)?,
        }

        for prop in &self.properties {
            write!(f, "{}", prop)?;
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl std::fmt::Display for TypeScriptProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Doc comment
        if let Some(comment) = &self.comment {
            writeln!(f, "  /** {} */", comment)?;
        }

        // Column name annotation if different from property name
        if self.name != self.db_name {
            writeln!(f, "  /** Column: {} */", self.db_name)?;
        }

        // Property definition
        let optional = if self.nullable { "?" } else { "" };
        let ts_type = if self.nullable {
            format!("{} | null", self.ts_type)
        } else {
            self.ts_type.clone()
        };

        writeln!(f, "  {}{}: {};", self.name, optional, ts_type)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_type_to_typescript() {
        let enums = IndexMap::new();
        let config = CodegenConfig::default();

        let generator = TypeScriptGenerator::default();

        // Integer types -> number
        assert_eq!(
            generator.sql_type_to_typescript("INTEGER", &enums, &config),
            "number"
        );
        assert_eq!(
            generator.sql_type_to_typescript("BIGINT", &enums, &config),
            "number"
        );

        // Text types -> string
        assert_eq!(
            generator.sql_type_to_typescript("TEXT", &enums, &config),
            "string"
        );
        assert_eq!(
            generator.sql_type_to_typescript("VARCHAR", &enums, &config),
            "string"
        );

        // Boolean
        assert_eq!(
            generator.sql_type_to_typescript("BOOLEAN", &enums, &config),
            "boolean"
        );

        // Timestamp -> Date
        assert_eq!(
            generator.sql_type_to_typescript("TIMESTAMP WITH TIME ZONE", &enums, &config),
            "Date"
        );

        // UUID -> string
        assert_eq!(
            generator.sql_type_to_typescript("UUID", &enums, &config),
            "string"
        );

        // JSON -> unknown
        assert_eq!(
            generator.sql_type_to_typescript("JSONB", &enums, &config),
            "unknown"
        );

        // Array types
        assert_eq!(
            generator.sql_type_to_typescript("INTEGER[]", &enums, &config),
            "number[]"
        );
        assert_eq!(
            generator.sql_type_to_typescript("TEXT[]", &enums, &config),
            "string[]"
        );
    }

    #[test]
    fn test_enum_to_string() {
        let ts_enum = TypeScriptEnum {
            name: "UserStatus".to_string(),
            db_name: "user_status".to_string(),
            members: vec![
                TypeScriptEnumMember {
                    name: "Active".to_string(),
                    db_value: "active".to_string(),
                },
                TypeScriptEnumMember {
                    name: "Inactive".to_string(),
                    db_value: "inactive".to_string(),
                },
            ],
            comment: Some("User account status".to_string()),
        };

        let output = ts_enum.to_string();
        assert!(output.contains("export enum UserStatus"));
        assert!(output.contains("Active = \"active\""));
        assert!(output.contains("Inactive = \"inactive\""));
        assert!(output.contains("/** User account status */"));
    }

    #[test]
    fn test_interface_to_string() {
        let ts_interface = TypeScriptInterface {
            name: "User".to_string(),
            table_name: "users".to_string(),
            properties: vec![
                TypeScriptProperty {
                    name: "id".to_string(),
                    db_name: "id".to_string(),
                    ts_type: "number".to_string(),
                    nullable: false,
                    comment: None,
                },
                TypeScriptProperty {
                    name: "email".to_string(),
                    db_name: "email".to_string(),
                    ts_type: "string".to_string(),
                    nullable: false,
                    comment: None,
                },
                TypeScriptProperty {
                    name: "createdAt".to_string(),
                    db_name: "created_at".to_string(),
                    ts_type: "Date".to_string(),
                    nullable: true,
                    comment: None,
                },
            ],
            comment: None,
            flavor: TypescriptFlavor::Interface,
        };

        let output = ts_interface.to_string();
        assert!(output.contains("export interface User"));
        assert!(output.contains("id: number;"));
        assert!(output.contains("email: string;"));
        assert!(output.contains("createdAt?: Date | null;"));
        assert!(output.contains("/** Column: created_at */"));
    }
}
