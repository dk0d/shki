//! Helper functions for parsing enum values from strings

use crate::schema::types::{DataType, ReferenceAction};
use crate::schema::IndexMethod;

/// Parse a string into a ReferenceAction
pub fn parse_referential_action(s: &str) -> ReferenceAction {
    match s.to_lowercase().as_str() {
        "cascade" => ReferenceAction::Cascade,
        "restrict" => ReferenceAction::Restrict,
        "set_null" | "setnull" => ReferenceAction::SetNull,
        "set_default" | "setdefault" => ReferenceAction::SetDefault,
        _ => ReferenceAction::NoAction,
    }
}

/// Parse a string into an IndexMethod
pub fn parse_index_method(s: &str) -> IndexMethod {
    match s.to_lowercase().as_str() {
        "hash" => IndexMethod::Hash,
        "gist" => IndexMethod::Gist,
        "spgist" => IndexMethod::SpGist,
        "gin" => IndexMethod::Gin,
        "brin" => IndexMethod::Brin,
        _ => IndexMethod::BTree,
    }
}

/// Parse a string into a DataType
pub fn parse_data_type(s: &str) -> DataType {
    match s.to_lowercase().as_str() {
        // Serial types
        "serial" => DataType::Serial,
        "bigserial" => DataType::BigSerial,
        "smallserial" => DataType::SmallSerial,

        // Integer types
        "integer" | "int" | "int4" => DataType::Integer,
        "bigint" | "int8" => DataType::BigInt,
        "smallint" | "int2" => DataType::SmallInt,

        // Text types
        "text" => DataType::Text,
        "varchar" => DataType::VarChar { length: None },
        "char" => DataType::Char { length: None },
        "citext" => DataType::Citext,

        // Boolean
        "boolean" | "bool" => DataType::Boolean,

        // Timestamp/Date/Time
        "timestamp" => DataType::Timestamp {
            precision: None,
            with_timezone: false,
        },
        "timestamptz" | "timestamp with time zone" => DataType::Timestamp {
            precision: None,
            with_timezone: true,
        },
        "date" => DataType::Date,
        "time" => DataType::Time {
            precision: None,
            with_timezone: false,
        },
        "timetz" | "time with time zone" => DataType::Time {
            precision: None,
            with_timezone: true,
        },
        "interval" => DataType::Interval,

        // UUID
        "uuid" => DataType::Uuid,

        // JSON
        "json" => DataType::Json,
        "jsonb" => DataType::JsonB,

        // Numeric/Decimal
        "numeric" => DataType::Numeric {
            precision: None,
            scale: None,
        },
        "decimal" => DataType::Decimal {
            precision: None,
            scale: None,
        },
        "money" => DataType::Money,
        "real" | "float4" => DataType::Real,
        "double precision" | "float8" | "double" => DataType::DoublePrecision,

        // Binary
        "bytea" => DataType::ByteA,
        "blob" => DataType::Blob,

        // Network types (PostgreSQL)
        "inet" => DataType::Inet,
        "cidr" => DataType::Cidr,
        "macaddr" => DataType::MacAddr,
        "macaddr8" => DataType::MacAddr8,

        // Geometric types (PostgreSQL)
        "point" => DataType::Point,
        "line" => DataType::Line,
        "lseg" => DataType::LSeg,
        "box" => DataType::Box,
        "path" => DataType::Path,
        "polygon" => DataType::Polygon,
        "circle" => DataType::Circle,

        // Range types (PostgreSQL)
        "int4range" => DataType::Int4Range,
        "int8range" => DataType::Int8Range,
        "numrange" => DataType::NumRange,
        "tsrange" => DataType::TsRange,
        "tstzrange" => DataType::TsTzRange,
        "daterange" => DataType::DateRange,

        _ => DataType::Text, // Default fallback
    }
}
