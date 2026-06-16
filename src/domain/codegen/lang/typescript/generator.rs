//! TypeScript code generator
//!
//! Generates TypeScript interfaces and enums from database schema snapshots.

use heck::ToUpperCamelCase;
use indexmap::IndexMap;

use crate::codegen::CodegenConfig;
use crate::codegen::generator::CodeGenerator;
use crate::models::iden::Iden;
use crate::schema::{Column, CompositeType, DataType, DbEnum, Table};
use crate::snapshots::Snapshot;

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
    /// TypeScript property name (matches the database column name)
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
    type EnumDef = TypeScriptEnum;
    type TableDef = TypeScriptInterface;

    fn init_output(&self, _config: &CodegenConfig) -> Self::Output {
        GeneratedTypeScript::default()
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
        self.build_interface(
            name,
            table_snapshot,
            &snapshot.enums(),
            &snapshot.composite_types(),
            config,
        )
    }

    fn generate_composite_type(
        &self,
        name: &Iden,
        composite_snapshot: &CompositeType,
        snapshot: &Snapshot,
        config: &CodegenConfig,
    ) -> Self::TableDef {
        self.build_composite_interface(
            name,
            composite_snapshot,
            &snapshot.enums(),
            &snapshot.composite_types(),
            config,
        )
    }

    fn insert_enum(&self, output: &mut Self::Output, name: &Iden, def: Self::EnumDef) {
        output.enums.insert(name.to_string(), def);
    }

    fn insert_table(&self, output: &mut Self::Output, name: &Iden, def: Self::TableDef) {
        output.interfaces.insert(name.to_string(), def);
    }
}

impl TypeScriptGenerator {
    /// Generate a TypeScript enum from an enum snapshot
    fn build_enum(
        &self,
        name: &str,
        enum_snapshot: &DbEnum,
        config: &CodegenConfig,
    ) -> TypeScriptEnum {
        let ts_name = self.transform_enum_name(name, config);

        let members = enum_snapshot
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
    fn build_interface(
        &self,
        name: &Iden,
        table: &Table,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> TypeScriptInterface {
        let ts_name = self.transform_struct_name(&name.name, config);

        let properties = table
            .columns
            .values()
            .map(|col| self.generate_property(col, enums, composites, config))
            .collect();

        TypeScriptInterface {
            name: ts_name,
            table_name: name.to_string(),
            properties,
            comment: table.comment.clone(),
            flavor: self.flavor,
        }
    }

    /// Generate a TypeScript interface from a composite type snapshot.
    ///
    /// Composite type attributes are not tracked as nullable in the schema
    /// model, so generated properties are emitted as non-optional.
    fn build_composite_interface(
        &self,
        name: &Iden,
        composite: &CompositeType,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> TypeScriptInterface {
        let ts_name = self.transform_composite_name(&name.name, config);

        let properties = composite
            .columns
            .iter()
            .map(|col| TypeScriptProperty {
                name: col.name.clone(),
                db_name: col.name.clone(),
                ts_type: self.sql_type_to_typescript(&col.data_type, enums, composites, config),
                nullable: false,
                comment: None,
            })
            .collect();

        TypeScriptInterface {
            name: ts_name,
            table_name: name.to_string(),
            properties,
            comment: composite.description.clone(),
            flavor: self.flavor,
        }
    }

    /// Generate a TypeScript property from a column snapshot
    fn generate_property(
        &self,
        col: &Column,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> TypeScriptProperty {
        let property_name = col.name.clone();
        let ts_type = self.sql_type_to_typescript(&col.data_type, enums, composites, config);

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
        sql_type: &DataType,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> String {
        if let Some(override_type) = self.overridden_type(sql_type, config) {
            return override_type.clone();
        }

        match sql_type {
            DataType::Boolean => "boolean".to_string(),

            DataType::SmallInt
            | DataType::Integer
            | DataType::BigInt
            | DataType::Serial
            | DataType::BigSerial
            | DataType::SmallSerial
            | DataType::Real
            | DataType::DoublePrecision
            | DataType::TinyInt { .. }
            | DataType::MediumInt { .. }
            | DataType::Year
            | DataType::SqliteInteger
            | DataType::SqliteReal => "number".to_string(),

            DataType::Numeric { .. } | DataType::Decimal { .. } | DataType::Money => {
                "string".to_string()
            }

            DataType::Char { .. }
            | DataType::VarChar { .. }
            | DataType::Text
            | DataType::Citext
            | DataType::Date
            | DataType::Time { .. }
            | DataType::Interval
            | DataType::Uuid
            | DataType::Inet
            | DataType::Cidr
            | DataType::MacAddr
            | DataType::MacAddr8
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
            | DataType::Set { .. } => "string".to_string(),

            DataType::Timestamp { .. } => "Date".to_string(),

            DataType::ByteA
            | DataType::Blob
            | DataType::Binary { .. }
            | DataType::VarBinary { .. }
            | DataType::TinyBlob
            | DataType::MediumBlob
            | DataType::LongBlob
            | DataType::SqliteBlob => "Uint8Array".to_string(),

            DataType::Json | DataType::JsonB => "unknown".to_string(),

            DataType::Array { element_type } => {
                format!(
                    "{}[]",
                    self.sql_type_to_typescript(element_type, enums, composites, config)
                )
            }

            DataType::Enum { name, schema } => self
                .custom_type_name(name, schema, enums, composites, config)
                .unwrap_or_else(|| "string".to_string()),

            DataType::Custom { name, schema } => self
                .custom_type_name(name, schema, enums, composites, config)
                .unwrap_or_else(|| "unknown".to_string()),

            DataType::Int4Range
            | DataType::Int8Range
            | DataType::NumRange
            | DataType::TsRange
            | DataType::TsTzRange
            | DataType::DateRange => "unknown".to_string(),
        }
    }
}

impl std::fmt::Display for GeneratedTypeScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for ts_enum in self.enums.values() {
            writeln!(f, "{}", ts_enum)?;
        }

        for ts_interface in self.interfaces.values() {
            writeln!(f, "{}", ts_interface)?;
        }

        Ok(())
    }
}

impl std::fmt::Display for TypeScriptEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        if let Some(comment) = &self.comment {
            writeln!(f, "/** {} */", comment)?;
        }

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
        if let Some(comment) = &self.comment {
            writeln!(f, "  /** {} */", comment)?;
        }

        if self.name != self.db_name {
            writeln!(f, "  /** Column: {} */", self.db_name)?;
        }

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
        let composites = IndexMap::new();
        let config = CodegenConfig::default();

        let generator = TypeScriptGenerator::default();

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::Integer, &enums, &composites, &config),
            "number"
        );
        assert_eq!(
            generator.sql_type_to_typescript(&DataType::BigInt, &enums, &composites, &config),
            "number"
        );

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::Text, &enums, &composites, &config),
            "string"
        );
        assert_eq!(
            generator.sql_type_to_typescript(&DataType::VarChar { length: None }, &enums, &composites, &config),
            "string"
        );

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::Boolean, &enums, &composites, &config),
            "boolean"
        );

        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Timestamp {
                    precision: None,
                    with_timezone: true,
                },
                &enums,
                &composites,
                &config,
            ),
            "Date"
        );

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::Uuid, &enums, &composites, &config),
            "string"
        );

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::JsonB, &enums, &composites, &config),
            "unknown"
        );

        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Array {
                    element_type: Box::new(DataType::Integer),
                },
                &enums,
                &composites,
                &config,
            ),
            "number[]"
        );
        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Array {
                    element_type: Box::new(DataType::Text),
                },
                &enums,
                &composites,
                &config,
            ),
            "string[]"
        );
    }

    #[test]
    fn test_sql_type_to_typescript_uses_overrides_and_enums() {
        let mut enums = IndexMap::new();
        enums.insert(
            Iden::new("user_status", Some("public".to_string())),
            DbEnum::with_values("user_status", vec!["active", "inactive"]),
        );
        let composites = IndexMap::new();
        let config = CodegenConfig::default().type_override("jsonb", "JsonValue");
        let generator = TypeScriptGenerator::default();

        assert_eq!(
            generator.sql_type_to_typescript(&DataType::JsonB, &enums, &composites, &config),
            "JsonValue"
        );
        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Enum {
                    name: "user_status".to_string(),
                    schema: Some("public".to_string()),
                },
                &enums,
                &composites,
                &config,
            ),
            "UserStatus"
        );
        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Custom {
                    name: "unknown_type".to_string(),
                    schema: None,
                },
                &enums,
                &composites,
                &config,
            ),
            "unknown"
        );
    }

    #[test]
    fn test_sql_type_to_typescript_resolves_composite_types() {
        let enums = IndexMap::new();
        let mut composites = IndexMap::new();
        composites.insert(
            Iden::new("address", Some("public".to_string())),
            CompositeType {
                name: "address".to_string(),
                schema: Some("public".to_string()),
                columns: vec![],
                description: None,
            },
        );
        let config = CodegenConfig::default();
        let generator = TypeScriptGenerator::default();

        assert_eq!(
            generator.sql_type_to_typescript(
                &DataType::Custom {
                    name: "address".to_string(),
                    schema: Some("public".to_string()),
                },
                &enums,
                &composites,
                &config,
            ),
            "Address"
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

    #[test]
    fn test_generate_interface_preserves_column_name_casing() {
        let mut table = Table::new("accounts");
        table.column(Column::new("UserID", DataType::Integer).not_null());
        table.column(Column::new(
            "created_at",
            DataType::Timestamp {
                precision: None,
                with_timezone: false,
            },
        ));
        table.column(Column::new("DisplayName", DataType::Text));

        let generator = TypeScriptGenerator::default();
        let ts_interface = generator.build_interface(
            &Iden::new("accounts", None),
            &table,
            &IndexMap::new(),
            &IndexMap::new(),
            &CodegenConfig::default(),
        );

        let output = ts_interface.to_string();
        assert!(output.contains("UserID: number;"));
        assert!(output.contains("created_at?: Date | null;"));
        assert!(output.contains("DisplayName?: string | null;"));
        assert!(!output.contains("userId"));
        assert!(!output.contains("createdAt"));
        assert!(!output.contains("displayName"));
    }

    #[test]
    fn test_type_flavor_interface_to_string() {
        let ts_interface = TypeScriptInterface {
            name: "User".to_string(),
            table_name: "users".to_string(),
            properties: vec![TypeScriptProperty {
                name: "id".to_string(),
                db_name: "id".to_string(),
                ts_type: "number".to_string(),
                nullable: false,
                comment: None,
            }],
            comment: None,
            flavor: TypescriptFlavor::Type,
        };

        let output = ts_interface.to_string();
        assert!(output.contains("export type User = {"));
        assert!(output.contains("id: number;"));
    }
}
