//! Protocol Buffer code generator
//!
//! Generates .proto files from database schema snapshots.

use heck::ToSnakeCase;
use indexmap::IndexMap;

use crate::codegen::CodegenConfig;
use crate::codegen::generator::CodeGenerator;
use crate::models::iden::Iden;
use crate::schema::{Column, CompositeType, DataType, DbEnum, Table};
use crate::snapshots::Snapshot;

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
        self.build_message(
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
        self.build_composite_message(
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
    fn build_enum(&self, name: &str, enum_snapshot: &DbEnum, config: &CodegenConfig) -> ProtoEnum {
        let proto_name = self.transform_enum_name(name, config);

        let prefix = proto_name.to_shouty_snake_case();

        let mut values = vec![ProtoEnumValue {
            name: format!("{}_UNSPECIFIED", prefix),
            db_value: String::new(),
            number: 0,
        }];

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
        name: &Iden,
        table: &Table,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> ProtoMessage {
        let proto_name = self.transform_struct_name(&name.name, config);

        let fields = table
            .columns
            .values()
            .enumerate()
            .map(|(i, col)| self.generate_field(col, i + 1, enums, composites, config))
            .collect();

        ProtoMessage {
            name: proto_name,
            table_name: name.to_string(),
            fields,
            comment: table.comment.clone(),
        }
    }

    /// Generate a Protocol Buffer message from a composite type snapshot.
    fn build_composite_message(
        &self,
        name: &Iden,
        composite: &CompositeType,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> ProtoMessage {
        let proto_name = self.transform_composite_name(&name.name, config);

        let fields = composite
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let (proto_type, repeated) =
                    self.sql_type_to_proto(&col.data_type, enums, composites, config);
                ProtoField {
                    name: col.name.to_snake_case(),
                    db_name: col.name.clone(),
                    proto_type,
                    number: (i + 1) as i32,
                    optional: false,
                    repeated,
                    comment: None,
                }
            })
            .collect();

        ProtoMessage {
            name: proto_name,
            table_name: name.to_string(),
            fields,
            comment: composite.description.clone(),
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
        col: &Column,
        field_number: usize,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> ProtoField {
        let field_name = col.name.to_snake_case();
        let (proto_type, repeated) =
            self.sql_type_to_proto(&col.data_type, enums, composites, config);

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

    fn sql_type_to_proto(
        &self,
        sql_type: &DataType,
        enums: &IndexMap<Iden, DbEnum>,
        composites: &IndexMap<Iden, CompositeType>,
        config: &CodegenConfig,
    ) -> (String, bool) {
        if let Some(override_type) = self.overridden_type(sql_type, config) {
            return (override_type.clone(), false);
        }

        match sql_type {
            DataType::Boolean => ("bool".to_string(), false),

            DataType::SmallInt
            | DataType::Integer
            | DataType::Serial
            | DataType::SmallSerial
            | DataType::TinyInt { .. }
            | DataType::MediumInt { .. }
            | DataType::Year
            | DataType::SqliteInteger => ("int32".to_string(), false),
            DataType::BigInt | DataType::BigSerial => ("int64".to_string(), false),

            DataType::Real | DataType::SqliteReal => ("float".to_string(), false),
            DataType::DoublePrecision => ("double".to_string(), false),

            DataType::Numeric { .. } | DataType::Decimal { .. } | DataType::Money => {
                ("string".to_string(), false)
            }

            DataType::Char { .. }
            | DataType::VarChar { .. }
            | DataType::Text
            | DataType::Citext
            | DataType::Date
            | DataType::Time { .. }
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
            | DataType::Set { .. } => ("string".to_string(), false),

            DataType::ByteA
            | DataType::Blob
            | DataType::Binary { .. }
            | DataType::VarBinary { .. }
            | DataType::TinyBlob
            | DataType::MediumBlob
            | DataType::LongBlob
            | DataType::SqliteBlob => ("bytes".to_string(), false),

            DataType::Uuid => ("string".to_string(), false),
            DataType::Json | DataType::JsonB => ("google.protobuf.Struct".to_string(), false),
            DataType::Timestamp { .. } => ("google.protobuf.Timestamp".to_string(), false),
            DataType::Interval => ("google.protobuf.Duration".to_string(), false),

            DataType::Array { element_type } => {
                let (inner_type, _) =
                    self.sql_type_to_proto(element_type, enums, composites, config);
                (inner_type, true)
            }

            DataType::Enum { name, schema } => {
                let proto_type = self
                    .custom_type_name(name, schema, enums, composites, config)
                    .unwrap_or_else(|| "string".to_string());
                (proto_type, false)
            }

            DataType::Custom { name, schema } => {
                let proto_type = self
                    .custom_type_name(name, schema, enums, composites, config)
                    .unwrap_or_else(|| "string".to_string());
                (proto_type, false)
            }

            DataType::Int4Range
            | DataType::Int8Range
            | DataType::NumRange
            | DataType::TsRange
            | DataType::TsTzRange
            | DataType::DateRange => ("string".to_string(), false),
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

        if !self.imports.is_empty() {
            writeln!(f)?;
            for import in &self.imports {
                writeln!(f, "import \"{}\";", import)?;
            }
        }

        if !self.enums.is_empty() || !self.messages.is_empty() {
            writeln!(f)?;
        }

        for proto_enum in self.enums.values() {
            write!(f, "{}", proto_enum)?;
        }

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

        if self.name != self.db_name {
            writeln!(f, "  // Column: {}", self.db_name)?;
        }

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
        let composites = IndexMap::new();
        let config = CodegenConfig::default();

        let generator = ProtobufGenerator;

        let (t, r) = generator.sql_type_to_proto(&DataType::Integer, &enums, &composites, &config);
        assert_eq!(t, "int32");
        assert!(!r);

        let (t, r) = generator.sql_type_to_proto(&DataType::BigInt, &enums, &composites, &config);
        assert_eq!(t, "int64");
        assert!(!r);

        let (t, r) = generator.sql_type_to_proto(&DataType::Text, &enums, &composites, &config);
        assert_eq!(t, "string");
        assert!(!r);

        let (t, r) = generator.sql_type_to_proto(
            &DataType::Timestamp {
                precision: None,
                with_timezone: true,
            },
            &enums,
            &composites,
            &config,
        );
        assert_eq!(t, "google.protobuf.Timestamp");
        assert!(!r);

        let (t, r) = generator.sql_type_to_proto(
            &DataType::Array {
                element_type: Box::new(DataType::Integer),
            },
            &enums,
            &composites,
            &config,
        );
        assert_eq!(t, "int32");
        assert!(r);
    }

    #[test]
    fn test_sql_type_to_proto_uses_overrides_and_enums() {
        let mut enums = IndexMap::new();
        enums.insert(
            Iden::new("user_status", Some("public".to_string())),
            DbEnum::with_values("user_status", vec!["active", "inactive"]),
        );
        let composites = IndexMap::new();
        let config = CodegenConfig::default().type_override("jsonb", "JsonValue");

        let generator = ProtobufGenerator;

        let (proto_type, repeated) =
            generator.sql_type_to_proto(&DataType::JsonB, &enums, &composites, &config);
        assert_eq!(proto_type, "JsonValue");
        assert!(!repeated);

        let (proto_type, repeated) = generator.sql_type_to_proto(
            &DataType::Enum {
                name: "user_status".to_string(),
                schema: Some("public".to_string()),
            },
            &enums,
            &composites,
            &config,
        );
        assert_eq!(proto_type, "UserStatus");
        assert!(!repeated);
    }

    #[test]
    fn test_sql_type_to_proto_resolves_composite_types() {
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
        let generator = ProtobufGenerator;

        let (proto_type, repeated) = generator.sql_type_to_proto(
            &DataType::Custom {
                name: "address".to_string(),
                schema: Some("public".to_string()),
            },
            &enums,
            &composites,
            &config,
        );
        assert_eq!(proto_type, "Address");
        assert!(!repeated);
    }

    #[test]
    fn test_generated_proto_collects_well_known_imports() {
        let mut table = Table::new("events");
        table.column(Column::new("payload", DataType::JsonB));
        table.column(Column::new(
            "created_at",
            DataType::Timestamp {
                precision: None,
                with_timezone: true,
            },
        ));

        let mut snapshot = Snapshot::new(crate::schema::SqlDialect::Postgres);
        snapshot.insert_table(Iden::new("events", None), table);

        let generator = ProtobufGenerator;

        let output = generator.generate(&snapshot, &CodegenConfig::default());

        assert_eq!(
            output.imports,
            vec![
                "google/protobuf/struct.proto".to_string(),
                "google/protobuf/timestamp.proto".to_string(),
            ]
        );
    }

    #[test]
    fn test_proto_field_repeated_takes_precedence_over_optional() {
        let field = ProtoField {
            name: "tags".to_string(),
            db_name: "tags".to_string(),
            proto_type: "string".to_string(),
            number: 1,
            optional: true,
            repeated: true,
            comment: None,
        };

        assert_eq!(field.to_string(), "  repeated string tags = 1;\n");
    }

    #[test]
    fn test_shouty_snake_case() {
        assert_eq!("user_status".to_shouty_snake_case(), "USER_STATUS");
        assert_eq!("UserStatus".to_shouty_snake_case(), "USER_STATUS");
        assert_eq!("active".to_shouty_snake_case(), "ACTIVE");
    }
}
