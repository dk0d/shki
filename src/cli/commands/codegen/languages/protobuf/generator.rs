//! Protocol Buffer code generator
//!
//! Generates .proto files from database schema snapshots.

use heck::ToSnakeCase;
use indexmap::IndexMap;

use crate::commands::codegen::CodegenConfig;
use crate::commands::codegen::languages::generator::CodeGenerator;
use crate::snapshot::{ColumnSnapshot, EnumSnapshot, Snapshot, TableSnapshot};

/// Generated Protocol Buffer code
#[derive(Debug, Clone, Default)]
pub struct GeneratedProto {
    /// Package name for the proto file
    pub package: String,
    /// Generated enum definitions
    pub enums: IndexMap<String, ProtoEnum>,
    /// Generated message definitions
    pub messages: IndexMap<String, ProtoMessage>,
    /// Required imports (e.g., "google/protobuf/timestamp.proto")
    pub imports: Vec<String>,
}

/// A generated Protocol Buffer enum
#[derive(Debug, Clone)]
pub struct ProtoEnum {
    /// Proto enum name (PascalCase)
    pub name: String,
    /// Original database enum name
    pub db_name: String,
    /// Enum values
    pub values: Vec<ProtoEnumValue>,
    /// Doc comment
    pub comment: Option<String>,
}

/// A value in a Protocol Buffer enum
#[derive(Debug, Clone)]
pub struct ProtoEnumValue {
    /// Proto value name (SCREAMING_SNAKE_CASE)
    pub name: String,
    /// Original database value
    pub db_value: String,
    /// Field number (0-indexed, with 0 reserved for UNSPECIFIED)
    pub number: i32,
}

/// A generated Protocol Buffer message
#[derive(Debug, Clone)]
pub struct ProtoMessage {
    /// Proto message name (PascalCase, singular)
    pub name: String,
    /// Original table name
    pub table_name: String,
    /// Message fields
    pub fields: Vec<ProtoField>,
    /// Doc comment
    pub comment: Option<String>,
}

/// A field in a Protocol Buffer message
#[derive(Debug, Clone)]
pub struct ProtoField {
    /// Proto field name (snake_case)
    pub name: String,
    /// Original column name
    pub db_name: String,
    /// Proto type (e.g., "int32", "string", "google.protobuf.Timestamp")
    pub proto_type: String,
    /// Field number (1-indexed)
    pub number: i32,
    /// Whether the field is optional
    pub optional: bool,
    /// Whether the field is repeated (array)
    pub repeated: bool,
    /// Doc comment
    pub comment: Option<String>,
}

/// Protocol Buffer generator
#[derive(Default)]
pub struct ProtobufGenerator;

impl ProtobufGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl CodeGenerator for ProtobufGenerator {
    type Output = GeneratedProto;
    type EnumDef = ProtoEnum;
    type TableDef = ProtoMessage;

    fn init_output(&self, _config: &CodegenConfig) -> Self::Output {
        GeneratedProto {
            package: "schema".to_string(),
            ..Default::default()
        }
    }

    fn generate_enum(
        &self,
        name: &str,
        enum_snapshot: &EnumSnapshot,
        config: &CodegenConfig,
    ) -> Self::EnumDef {
        self.build_enum(name, enum_snapshot, config)
    }

    fn generate_table(
        &self,
        name: &str,
        table_snapshot: &TableSnapshot,
        snapshot: &Snapshot,
        config: &CodegenConfig,
    ) -> Self::TableDef {
        self.build_message(name, table_snapshot, &snapshot.enums, config)
    }

    fn insert_enum(&self, output: &mut Self::Output, name: &str, def: Self::EnumDef) {
        output.enums.insert(name.to_string(), def);
    }

    fn insert_table(&self, output: &mut Self::Output, name: &str, def: Self::TableDef) {
        for import in self.collect_message_imports(&def) {
            if !output.imports.contains(&import) {
                output.imports.push(import);
            }
        }
        output.messages.insert(name.to_string(), def);
        output.imports.sort();
    }
}

impl ProtobufGenerator {
    /// Generate a Protocol Buffer enum from an enum snapshot
    fn build_enum(
        &self,
        name: &str,
        enum_snapshot: &EnumSnapshot,
        config: &CodegenConfig,
    ) -> ProtoEnum {
        let proto_name = self.transform_enum_name(name, config);

        // Create UNSPECIFIED as the first value (proto3 requirement)
        let prefix = proto_name.to_shouty_snake_case();

        let mut values = vec![ProtoEnumValue {
            name: format!("{}_UNSPECIFIED", prefix),
            db_value: String::new(),
            number: 0,
        }];

        // Add actual enum values starting from 1
        for (i, value) in enum_snapshot.values.iter().enumerate() {
            let value_name = format!("{}_{}", prefix, value.to_shouty_snake_case());
            values.push(ProtoEnumValue {
                name: value_name,
                db_value: value.clone(),
                number: (i + 1) as i32,
            });
        }

        ProtoEnum {
            name: proto_name,
            db_name: name.to_string(),
            values,
            comment: enum_snapshot.description.clone(),
        }
    }

    /// Generate a Protocol Buffer message from a table snapshot
    fn build_message(
        &self,
        name: &str,
        table: &TableSnapshot,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> ProtoMessage {
        let proto_name = self.transform_struct_name(name, config);

        let fields: Vec<ProtoField> = table
            .columns
            .values()
            .enumerate()
            .map(|(i, col)| self.generate_field(col, i + 1, enums, config))
            .collect();

        ProtoMessage {
            name: proto_name,
            table_name: name.to_string(),
            fields,
            comment: table.comment.clone(),
        }
    }

    fn collect_message_imports(&self, message: &ProtoMessage) -> Vec<String> {
        let mut imports = Vec::new();

        for field in &message.fields {
            if field.proto_type.starts_with("google.protobuf.") {
                let import = match field.proto_type.as_str() {
                    "google.protobuf.Timestamp" => "google/protobuf/timestamp.proto",
                    "google.protobuf.Duration" => "google/protobuf/duration.proto",
                    "google.protobuf.Struct" => "google/protobuf/struct.proto",
                    "google.protobuf.Any" => "google/protobuf/any.proto",
                    _ => continue,
                };
                imports.push(import.to_string());
            }
        }

        imports.sort();
        imports.dedup();
        imports
    }

    /// Generate a Protocol Buffer field from a column snapshot
    fn generate_field(
        &self,
        col: &ColumnSnapshot,
        field_number: usize,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> ProtoField {
        let field_name = col.name.to_snake_case();
        let (proto_type, repeated) = self.sql_type_to_proto(&col.data_type, enums, config);

        ProtoField {
            name: field_name,
            db_name: col.name.clone(),
            proto_type,
            number: field_number as i32,
            optional: col.nullable,
            repeated,
            comment: col.comment.clone(),
        }
    }

    /// Convert a SQL type to a Protocol Buffer type
    ///
    /// Returns (proto_type, is_repeated)
    fn sql_type_to_proto(
        &self,
        sql_type: &str,
        enums: &IndexMap<String, EnumSnapshot>,
        config: &CodegenConfig,
    ) -> (String, bool) {
        // Check for type overrides first
        let normalized = self.normalized_sql_type(sql_type);
        if let Some(override_type) = self.overridden_type(&normalized, config) {
            return (override_type.clone(), false);
        }

        if let Some(enum_name) = self.enum_type_name(sql_type, enums) {
            return (enum_name, false);
        }

        // Handle array types
        if let Some(inner) = sql_type.strip_suffix("[]") {
            let (inner_type, _) = self.sql_type_to_proto(inner, enums, config);
            return (inner_type, true);
        }

        // Map SQL types to Protocol Buffer types
        match normalized.as_str() {
            // Boolean
            "bool" | "boolean" => ("bool".to_string(), false),

            // Integers
            "smallint" | "int2" | "smallserial" => ("int32".to_string(), false),
            "integer" | "int" | "int4" | "serial" => ("int32".to_string(), false),
            "bigint" | "int8" | "bigserial" => ("int64".to_string(), false),

            // Floating point
            "real" | "float4" => ("float".to_string(), false),
            "double precision" | "float8" => ("double".to_string(), false),

            // Numeric/Decimal - use string to preserve precision
            "numeric" | "decimal" | "money" => ("string".to_string(), false),

            // Text types
            "text" | "varchar" | "char" | "character varying" | "character" | "citext" | "name" => {
                ("string".to_string(), false)
            }

            // Binary
            "bytea" | "blob" => ("bytes".to_string(), false),

            // UUID - use string representation
            "uuid" => ("string".to_string(), false),

            // JSON - use google.protobuf.Struct or string
            "json" | "jsonb" => ("google.protobuf.Struct".to_string(), false),

            // Date/Time - use google.protobuf.Timestamp
            "timestamp"
            | "timestamp without time zone"
            | "timestamp with time zone"
            | "timestamptz"
            | "datetime" => ("google.protobuf.Timestamp".to_string(), false),

            // Date - use string (ISO 8601 format) or custom message
            "date" => ("string".to_string(), false),

            // Time - use string (ISO 8601 format)
            "time" | "time without time zone" | "time with time zone" => {
                ("string".to_string(), false)
            }

            // Interval - use google.protobuf.Duration
            "interval" => ("google.protobuf.Duration".to_string(), false),

            // Network types - use string
            "inet" | "cidr" | "macaddr" | "macaddr8" => ("string".to_string(), false),

            // Geometric types - use string or custom messages
            "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => {
                ("string".to_string(), false)
            }

            // Text search types - use string
            "tsquery" | "tsvector" => ("string".to_string(), false),

            // XML - use string
            "xml" => ("string".to_string(), false),

            // MySQL specific
            "tinyint" => ("int32".to_string(), false),
            "mediumint" => ("int32".to_string(), false),
            "year" => ("int32".to_string(), false),
            "enum" => ("string".to_string(), false), // MySQL inline enums

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
                    ("string".to_string(), false)
                } else if normalized.starts_with("timestamp") {
                    ("google.protobuf.Timestamp".to_string(), false)
                } else if normalized.starts_with("time") {
                    ("string".to_string(), false)
                } else {
                    // Fallback to string for unknown types
                    ("string".to_string(), false)
                }
            }
        }
    }
}

// Helper trait for SCREAMING_SNAKE_CASE conversion
trait ToShoutySnakeCase {
    fn to_shouty_snake_case(&self) -> String;
}

impl ToShoutySnakeCase for str {
    fn to_shouty_snake_case(&self) -> String {
        self.to_snake_case().to_uppercase()
    }
}

impl std::fmt::Display for GeneratedProto {
    /// Format the generated proto as a complete .proto file
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "syntax = \"proto3\";")?;
        writeln!(f, "package {};", self.package)?;

        // Imports
        if !self.imports.is_empty() {
            writeln!(f)?;
            for import in &self.imports {
                writeln!(f, "import \"{}\";", import)?;
            }
        }

        if !self.enums.is_empty() || !self.messages.is_empty() {
            writeln!(f)?;
        }

        // Enums
        for proto_enum in self.enums.values() {
            write!(f, "{}", proto_enum)?;
        }

        // Messages
        for proto_message in self.messages.values() {
            write!(f, "{}", proto_message)?;
        }

        Ok(())
    }
}

impl GeneratedProto {
    /// Check if there's any generated code
    pub fn is_empty(&self) -> bool {
        self.enums.is_empty() && self.messages.is_empty()
    }
}

impl std::fmt::Display for ProtoEnum {
    /// Format the enum as Protocol Buffer syntax
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(comment) = &self.comment {
            writeln!(f, "// {}", comment)?;
        }

        writeln!(f, "enum {} {{", self.name)?;

        for value in &self.values {
            if !value.db_value.is_empty() {
                writeln!(f, "  // DB value: \"{}\"", value.db_value)?;
            }
            writeln!(f, "  {} = {};", value.name, value.number)?;
        }

        writeln!(f, "}}")
    }
}

impl std::fmt::Display for ProtoMessage {
    /// Format the message as Protocol Buffer syntax
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(comment) = &self.comment {
            writeln!(f, "// {}", comment)?;
        }

        // Table name annotation
        writeln!(f, "// Table: {}", self.table_name)?;
        writeln!(f, "message {} {{", self.name)?;

        for field in &self.fields {
            write!(f, "{}", field)?;
        }

        writeln!(f, "}}")
    }
}

impl std::fmt::Display for ProtoField {
    /// Format the field as Protocol Buffer syntax
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(comment) = &self.comment {
            writeln!(f, "  // {}", comment)?;
        }

        // Column name annotation if different
        if self.name != self.db_name {
            writeln!(f, "  // Column: {}", self.db_name)?;
        }

        // Field definition
        let modifier = if self.repeated {
            "repeated "
        } else if self.optional {
            "optional "
        } else {
            ""
        };

        writeln!(
            f,
            "  {}{} {} = {};",
            modifier, self.proto_type, self.name, self.number
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_type_to_proto() {
        let enums = IndexMap::new();
        let config = CodegenConfig::default();

        // Integer types
        let generator = ProtobufGenerator::default();

        let (t, r) = generator.sql_type_to_proto("INTEGER", &enums, &config);
        assert_eq!(t, "int32");
        assert!(!r);

        let (t, r) = generator.sql_type_to_proto("BIGINT", &enums, &config);
        assert_eq!(t, "int64");
        assert!(!r);

        // Text types
        let (t, r) = generator.sql_type_to_proto("TEXT", &enums, &config);
        assert_eq!(t, "string");
        assert!(!r);

        // Timestamp
        let (t, r) = generator.sql_type_to_proto("TIMESTAMP WITH TIME ZONE", &enums, &config);
        assert_eq!(t, "google.protobuf.Timestamp");
        assert!(!r);

        // Array types
        let (t, r) = generator.sql_type_to_proto("INTEGER[]", &enums, &config);
        assert_eq!(t, "int32");
        assert!(r);
    }

    #[test]
    fn test_shouty_snake_case() {
        assert_eq!("user_status".to_shouty_snake_case(), "USER_STATUS");
        assert_eq!("UserStatus".to_shouty_snake_case(), "USER_STATUS");
        assert_eq!("active".to_shouty_snake_case(), "ACTIVE");
    }
}
