//! SQL data types for different dialects

use serde::{Deserialize, Serialize};

use crate::sql::generator::{SqlStmt, ToSql};

use super::SqlDialect;

/// Enum type definition (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbEnum {
    pub name: String,
    pub schema: Option<String>,
    pub values: Vec<String>,
    /// Description/comment for the enum (used for SQL comments and Rust doc comments)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl DbEnum {
    /// Create a new enum type with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            values: Vec::new(),
            description: None,
        }
    }

    /// Create a new enum with name and values
    pub fn with_values(name: impl Into<String>, values: Vec<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            values: values.into_iter().map(Into::into).collect(),
            description: None,
        }
    }

    /// Add a value to the enum
    pub fn add_value(&mut self, value: impl Into<String>) -> &mut Self {
        self.values.push(value.into());
        self
    }

    /// Set the schema (usually set automatically when added to a Schema)
    pub fn in_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }
}

/// SQL data type representation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum DataType {
    // Numeric types
    SmallInt,
    Integer,
    BigInt,
    Serial,
    BigSerial,
    SmallSerial,
    Real,
    DoublePrecision,
    Numeric {
        precision: Option<u32>,
        scale: Option<u32>,
    },
    Decimal {
        precision: Option<u32>,
        scale: Option<u32>,
    },
    Money,

    // Character types
    Char {
        length: Option<u32>,
    },
    VarChar {
        length: Option<u32>,
    },
    Text,
    Citext, // PostgreSQL extension

    // Binary types
    ByteA,
    Blob,
    Binary {
        length: Option<u32>,
    },
    VarBinary {
        length: Option<u32>,
    },

    // Boolean
    Boolean,

    // Date/Time types
    Date,
    Time {
        precision: Option<u32>,
        with_timezone: bool,
    },
    Timestamp {
        precision: Option<u32>,
        with_timezone: bool,
    },
    Interval,

    // UUID
    Uuid,

    // JSON types
    Json,
    JsonB,

    // Network types (PostgreSQL)
    Inet,
    Cidr,
    MacAddr,
    MacAddr8,

    // Geometric types (PostgreSQL)
    Point,
    Line,
    LSeg,
    Box,
    Path,
    Polygon,
    Circle,

    // Range types (PostgreSQL)
    Int4Range,
    Int8Range,
    NumRange,
    TsRange,
    TsTzRange,
    DateRange,

    // Array type (PostgreSQL)
    Array {
        element_type: Box<DataType>,
    },

    // Enum type (reference to a custom enum)
    Enum {
        name: String,
        schema: Option<String>,
    },

    // Custom/user-defined type
    Custom {
        name: String,
        schema: Option<String>,
    },

    // MySQL specific
    TinyInt {
        unsigned: bool,
    },
    MediumInt {
        unsigned: bool,
    },
    Year,
    Enum_ {
        values: Vec<String>,
    }, // MySQL inline enum
    Set {
        values: Vec<String>,
    },
    TinyText,
    MediumText,
    LongText,
    TinyBlob,
    MediumBlob,
    LongBlob,

    // SQLite types (affinity)
    SqliteInteger,
    SqliteReal,
    SqliteText,
    SqliteBlob,
}

impl ToSql for DataType {
    fn to_sql(&self, dialect: &SqlDialect) -> crate::Result<SqlStmt> {
        Ok(self.to_string(dialect).into())
    }
}

impl DataType {
    /// Get the SQL representation for PostgreSQL
    pub fn to_postgres_sql(&self) -> String {
        match self {
            DataType::SmallInt => "SMALLINT".to_string(),
            DataType::Integer => "INTEGER".to_string(),
            DataType::BigInt => "BIGINT".to_string(),
            DataType::Serial => "SERIAL".to_string(),
            DataType::BigSerial => "BIGSERIAL".to_string(),
            DataType::SmallSerial => "SMALLSERIAL".to_string(),
            DataType::Real => "REAL".to_string(),
            DataType::DoublePrecision => "DOUBLE PRECISION".to_string(),
            DataType::Numeric { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("NUMERIC({}, {})", p, s),
                (Some(p), None) => format!("NUMERIC({})", p),
                _ => "NUMERIC".to_string(),
            },
            DataType::Decimal { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("DECIMAL({}, {})", p, s),
                (Some(p), None) => format!("DECIMAL({})", p),
                _ => "DECIMAL".to_string(),
            },
            DataType::Money => "MONEY".to_string(),
            DataType::Char { length } => match length {
                Some(l) => format!("CHAR({})", l),
                None => "CHAR(1)".to_string(),
            },
            DataType::VarChar { length } => match length {
                Some(l) => format!("VARCHAR({})", l),
                None => "VARCHAR".to_string(),
            },
            DataType::Text => "TEXT".to_string(),
            DataType::Citext => "CITEXT".to_string(),
            DataType::ByteA => "BYTEA".to_string(),
            DataType::Boolean => "BOOLEAN".to_string(),
            DataType::Date => "DATE".to_string(),
            DataType::Time {
                precision,
                with_timezone,
            } => {
                let tz = if *with_timezone {
                    " WITH TIME ZONE"
                } else {
                    ""
                };
                match precision {
                    Some(p) => format!("TIME({}){}", p, tz),
                    None => format!("TIME{}", tz),
                }
            }
            DataType::Timestamp {
                precision,
                with_timezone,
            } => {
                let tz = if *with_timezone {
                    " WITH TIME ZONE"
                } else {
                    ""
                };
                match precision {
                    Some(p) => format!("TIMESTAMP({}){}", p, tz),
                    None => format!("TIMESTAMP{}", tz),
                }
            }
            DataType::Interval => "INTERVAL".to_string(),
            DataType::Uuid => "UUID".to_string(),
            DataType::Json => "JSON".to_string(),
            DataType::JsonB => "JSONB".to_string(),
            DataType::Inet => "INET".to_string(),
            DataType::Cidr => "CIDR".to_string(),
            DataType::MacAddr => "MACADDR".to_string(),
            DataType::MacAddr8 => "MACADDR8".to_string(),
            DataType::Point => "POINT".to_string(),
            DataType::Line => "LINE".to_string(),
            DataType::LSeg => "LSEG".to_string(),
            DataType::Box => "BOX".to_string(),
            DataType::Path => "PATH".to_string(),
            DataType::Polygon => "POLYGON".to_string(),
            DataType::Circle => "CIRCLE".to_string(),
            DataType::Int4Range => "INT4RANGE".to_string(),
            DataType::Int8Range => "INT8RANGE".to_string(),
            DataType::NumRange => "NUMRANGE".to_string(),
            DataType::TsRange => "TSRANGE".to_string(),
            DataType::TsTzRange => "TSTZRANGE".to_string(),
            DataType::DateRange => "DATERANGE".to_string(),
            DataType::Array { element_type } => format!("{}[]", element_type.to_postgres_sql()),
            DataType::Enum { name, schema } => match schema {
                Some(s) => format!("\"{}\".\"{}\"", s, name),
                None => format!("\"{}\"", name),
            },
            DataType::Custom { name, schema } => match schema {
                Some(s) => format!("\"{}\".\"{}\"", s, name),
                None => format!("\"{}\"", name),
            },
            // MySQL/SQLite types that might appear
            _ => "TEXT".to_string(),
        }
    }

    /// Get the SQL representation for MySQL
    pub fn to_mysql_sql(&self) -> String {
        match self {
            DataType::SmallInt => "SMALLINT".to_string(),
            DataType::Integer => "INT".to_string(),
            DataType::BigInt => "BIGINT".to_string(),
            DataType::Serial => "INT AUTO_INCREMENT".to_string(),
            DataType::BigSerial => "BIGINT AUTO_INCREMENT".to_string(),
            DataType::Real => "FLOAT".to_string(),
            DataType::DoublePrecision => "DOUBLE".to_string(),
            DataType::Numeric { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("DECIMAL({}, {})", p, s),
                (Some(p), None) => format!("DECIMAL({})", p),
                _ => "DECIMAL".to_string(),
            },
            DataType::Decimal { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("DECIMAL({}, {})", p, s),
                (Some(p), None) => format!("DECIMAL({})", p),
                _ => "DECIMAL".to_string(),
            },
            DataType::Char { length } => match length {
                Some(l) => format!("CHAR({})", l),
                None => "CHAR(1)".to_string(),
            },
            DataType::VarChar { length } => match length {
                Some(l) => format!("VARCHAR({})", l),
                None => "VARCHAR(255)".to_string(),
            },
            DataType::Text => "TEXT".to_string(),
            DataType::TinyText => "TINYTEXT".to_string(),
            DataType::MediumText => "MEDIUMTEXT".to_string(),
            DataType::LongText => "LONGTEXT".to_string(),
            DataType::ByteA | DataType::Blob => "BLOB".to_string(),
            DataType::TinyBlob => "TINYBLOB".to_string(),
            DataType::MediumBlob => "MEDIUMBLOB".to_string(),
            DataType::LongBlob => "LONGBLOB".to_string(),
            DataType::Binary { length } => match length {
                Some(l) => format!("BINARY({})", l),
                None => "BINARY(1)".to_string(),
            },
            DataType::VarBinary { length } => match length {
                Some(l) => format!("VARBINARY({})", l),
                None => "VARBINARY(255)".to_string(),
            },
            DataType::Boolean => "TINYINT(1)".to_string(),
            DataType::Date => "DATE".to_string(),
            DataType::Time { precision, .. } => match precision {
                Some(p) => format!("TIME({})", p),
                None => "TIME".to_string(),
            },
            DataType::Timestamp { precision, .. } => match precision {
                Some(p) => format!("TIMESTAMP({})", p),
                None => "TIMESTAMP".to_string(),
            },
            DataType::Uuid => "CHAR(36)".to_string(),
            DataType::Json | DataType::JsonB => "JSON".to_string(),
            DataType::TinyInt { unsigned } => if *unsigned {
                "TINYINT UNSIGNED"
            } else {
                "TINYINT"
            }
            .to_string(),
            DataType::MediumInt { unsigned } => if *unsigned {
                "MEDIUMINT UNSIGNED"
            } else {
                "MEDIUMINT"
            }
            .to_string(),
            DataType::Year => "YEAR".to_string(),
            DataType::Enum_ { values } => {
                let vals: Vec<_> = values.iter().map(|v| format!("'{}'", v)).collect();
                format!("ENUM({})", vals.join(", "))
            }
            DataType::Set { values } => {
                let vals: Vec<_> = values.iter().map(|v| format!("'{}'", v)).collect();
                format!("SET({})", vals.join(", "))
            }
            _ => "TEXT".to_string(),
        }
    }

    /// Get the SQL representation for SQLite
    pub fn to_sqlite_sql(&self) -> String {
        match self {
            DataType::SmallInt
            | DataType::Integer
            | DataType::BigInt
            | DataType::Serial
            | DataType::BigSerial
            | DataType::SmallSerial
            | DataType::TinyInt { .. }
            | DataType::MediumInt { .. }
            | DataType::SqliteInteger => "INTEGER".to_string(),

            DataType::Real
            | DataType::DoublePrecision
            | DataType::Numeric { .. }
            | DataType::Decimal { .. }
            | DataType::SqliteReal => "REAL".to_string(),

            DataType::Char { .. }
            | DataType::VarChar { .. }
            | DataType::Text
            | DataType::TinyText
            | DataType::MediumText
            | DataType::LongText
            | DataType::Uuid
            | DataType::Json
            | DataType::JsonB
            | DataType::SqliteText => "TEXT".to_string(),

            DataType::ByteA
            | DataType::Blob
            | DataType::Binary { .. }
            | DataType::VarBinary { .. }
            | DataType::TinyBlob
            | DataType::MediumBlob
            | DataType::LongBlob
            | DataType::SqliteBlob => "BLOB".to_string(),

            DataType::Boolean => "INTEGER".to_string(),
            DataType::Date => "TEXT".to_string(),
            DataType::Time { .. } => "TEXT".to_string(),
            DataType::Timestamp { .. } => "TEXT".to_string(),

            _ => "TEXT".to_string(),
        }
    }

    pub fn to_string(&self, dialect: &SqlDialect) -> String {
        match dialect {
            SqlDialect::Postgres => self.to_postgres_sql(),
            SqlDialect::Sqlite => self.to_sqlite_sql(),
            SqlDialect::Mysql => self.to_mysql_sql(),
        }
    }
}

impl DataType {
    pub fn parse(value: impl Into<String>, dialect: &SqlDialect) -> Self {
        (dialect, value.into().as_str()).into()
    }
}

impl From<(SqlDialect, String)> for DataType {
    fn from((dialect, value): (SqlDialect, String)) -> Self {
        Self::from((dialect, value.as_str()))
    }
}

impl From<(SqlDialect, &str)> for DataType {
    fn from((dialect, value): (SqlDialect, &str)) -> Self {
        match dialect {
            SqlDialect::Postgres => parse_postgres_type(value),
            SqlDialect::Mysql => parse_mysql_type(value),
            SqlDialect::Sqlite => parse_sqlite_type(value),
        }
    }
}

impl From<(&SqlDialect, &str)> for DataType {
    fn from((dialect, value): (&SqlDialect, &str)) -> Self {
        match dialect {
            SqlDialect::Postgres => parse_postgres_type(value),
            SqlDialect::Mysql => parse_mysql_type(value),
            SqlDialect::Sqlite => parse_sqlite_type(value),
        }
    }
}

fn parse_postgres_type(value: &str) -> DataType {
    let ty = value.trim();
    let normalized = normalize_type_name(ty);

    if let Some(element_type) = normalized.strip_suffix("[]") {
        return DataType::Array {
            element_type: Box::new(parse_postgres_type(element_type.trim())),
        };
    }

    if let Some(length) = parse_type_length(ty, &["CHARACTER VARYING", "VARCHAR"]) {
        return DataType::VarChar { length };
    }

    if let Some(length) = parse_type_length(ty, &["CHARACTER", "CHAR"]) {
        return DataType::Char { length };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "NUMERIC") {
        return DataType::Numeric { precision, scale };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "DECIMAL") {
        return DataType::Decimal { precision, scale };
    }

    if let Some(parsed) = parse_postgres_time_type(ty) {
        return parsed;
    }

    let core = match normalized.as_str() {
        "SMALLINT" | "INT2" => Some(DataType::SmallInt),
        "INTEGER" | "INT" | "INT4" => Some(DataType::Integer),
        "BIGINT" | "INT8" => Some(DataType::BigInt),
        "SERIAL" | "SERIAL4" => Some(DataType::Serial),
        "BIGSERIAL" | "SERIAL8" => Some(DataType::BigSerial),
        "SMALLSERIAL" | "SERIAL2" => Some(DataType::SmallSerial),
        "REAL" | "FLOAT4" => Some(DataType::Real),
        "DOUBLE PRECISION" | "FLOAT8" => Some(DataType::DoublePrecision),
        "MONEY" => Some(DataType::Money),
        "TEXT" => Some(DataType::Text),
        "CITEXT" => Some(DataType::Citext),
        "BYTEA" => Some(DataType::ByteA),
        "BOOLEAN" | "BOOL" => Some(DataType::Boolean),
        "DATE" => Some(DataType::Date),
        "INTERVAL" => Some(DataType::Interval),
        "UUID" => Some(DataType::Uuid),
        "JSON" => Some(DataType::Json),
        "JSONB" => Some(DataType::JsonB),
        "INET" => Some(DataType::Inet),
        "CIDR" => Some(DataType::Cidr),
        "MACADDR" => Some(DataType::MacAddr),
        "MACADDR8" => Some(DataType::MacAddr8),
        "POINT" => Some(DataType::Point),
        "LINE" => Some(DataType::Line),
        "LSEG" => Some(DataType::LSeg),
        "BOX" => Some(DataType::Box),
        "PATH" => Some(DataType::Path),
        "POLYGON" => Some(DataType::Polygon),
        "CIRCLE" => Some(DataType::Circle),
        "INT4RANGE" => Some(DataType::Int4Range),
        "INT8RANGE" => Some(DataType::Int8Range),
        "NUMRANGE" => Some(DataType::NumRange),
        "TSRANGE" => Some(DataType::TsRange),
        "TSTZRANGE" => Some(DataType::TsTzRange),
        "DATERANGE" => Some(DataType::DateRange),
        _ => None,
    };

    if let Some(core) = core {
        return core;
    }

    if let Some((name, schema)) = parse_qualified_identifier(ty) {
        return DataType::Custom { name, schema };
    }

    DataType::Custom {
        name: ty.to_string(),
        schema: None,
    }
}

fn parse_mysql_type(value: &str) -> DataType {
    let ty = value.trim();
    let normalized = normalize_type_name(ty);

    if let Some(values) = parse_mysql_value_list(ty, "ENUM") {
        return DataType::Enum_ { values };
    }

    if let Some(values) = parse_mysql_value_list(ty, "SET") {
        return DataType::Set { values };
    }

    if let Some(length) = parse_type_length(ty, &["VARCHAR"]) {
        return DataType::VarChar { length };
    }

    if let Some(length) = parse_type_length(ty, &["CHAR"]) {
        return DataType::Char { length };
    }

    if let Some(length) = parse_type_length(ty, &["BINARY"]) {
        return DataType::Binary { length };
    }

    if let Some(length) = parse_type_length(ty, &["VARBINARY"]) {
        return DataType::VarBinary { length };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "NUMERIC") {
        return DataType::Numeric { precision, scale };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "DECIMAL") {
        return DataType::Decimal { precision, scale };
    }

    if let Some(parsed) = parse_mysql_time_type(ty) {
        return parsed;
    }

    if normalized == "INT AUTO_INCREMENT" || normalized == "INTEGER AUTO_INCREMENT" {
        return DataType::Serial;
    }

    if normalized == "BIGINT AUTO_INCREMENT" {
        return DataType::BigSerial;
    }

    if normalized == "BOOLEAN" || normalized == "BOOL" {
        return DataType::Boolean;
    }

    if normalized.starts_with("TINYINT") {
        if parse_type_length(ty, &["TINYINT"]) == Some(Some(1)) && !normalized.contains("UNSIGNED")
        {
            return DataType::Boolean;
        }

        return DataType::TinyInt {
            unsigned: normalized.contains("UNSIGNED"),
        };
    }

    if normalized.starts_with("MEDIUMINT") {
        return DataType::MediumInt {
            unsigned: normalized.contains("UNSIGNED"),
        };
    }

    match normalized.as_str() {
        "SMALLINT" => DataType::SmallInt,
        "INT" | "INTEGER" => DataType::Integer,
        "BIGINT" => DataType::BigInt,
        "FLOAT" => DataType::Real,
        "DOUBLE" | "DOUBLE PRECISION" => DataType::DoublePrecision,
        "TEXT" => DataType::Text,
        "TINYTEXT" => DataType::TinyText,
        "MEDIUMTEXT" => DataType::MediumText,
        "LONGTEXT" => DataType::LongText,
        "BLOB" => DataType::Blob,
        "TINYBLOB" => DataType::TinyBlob,
        "MEDIUMBLOB" => DataType::MediumBlob,
        "LONGBLOB" => DataType::LongBlob,
        "DATE" => DataType::Date,
        "YEAR" => DataType::Year,
        "JSON" => DataType::Json,
        _ => DataType::Custom {
            name: ty.to_string(),
            schema: None,
        },
    }
}

fn parse_sqlite_type(value: &str) -> DataType {
    let ty = value.trim();
    let normalized = normalize_type_name(ty);

    if let Some(length) = parse_type_length(ty, &["VARCHAR"]) {
        return DataType::VarChar { length };
    }

    if let Some(length) = parse_type_length(ty, &["CHAR"]) {
        return DataType::Char { length };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "NUMERIC") {
        return DataType::Numeric { precision, scale };
    }

    if let Some((precision, scale)) = parse_precision_and_scale(ty, "DECIMAL") {
        return DataType::Decimal { precision, scale };
    }

    if let Some(parsed) = parse_mysql_time_type(ty) {
        return parsed;
    }

    match normalized.as_str() {
        "INTEGER" => DataType::SqliteInteger,
        "REAL" => DataType::SqliteReal,
        "TEXT" => DataType::SqliteText,
        "BLOB" => DataType::SqliteBlob,
        "BOOLEAN" => DataType::Boolean,
        "DATE" => DataType::Date,
        "JSON" => DataType::Json,
        "JSONB" => DataType::JsonB,
        _ if normalized.contains("INT") => DataType::Integer,
        _ if normalized.contains("CHAR")
            || normalized.contains("CLOB")
            || normalized.contains("TEXT") =>
        {
            DataType::Text
        }
        _ if normalized.contains("BLOB") => DataType::Blob,
        _ if normalized.contains("REAL")
            || normalized.contains("FLOA")
            || normalized.contains("DOUB") =>
        {
            DataType::Real
        }
        _ => DataType::Custom {
            name: ty.to_string(),
            schema: None,
        },
    }
}

fn parse_postgres_time_type(ty: &str) -> Option<DataType> {
    let normalized = normalize_type_name(ty);

    if normalized == "TIMESTAMPTZ" {
        return Some(DataType::Timestamp {
            precision: None,
            with_timezone: true,
        });
    }

    if normalized == "TIMETZ" {
        return Some(DataType::Time {
            precision: None,
            with_timezone: true,
        });
    }

    if normalized.starts_with("TIMESTAMP") {
        return Some(DataType::Timestamp {
            precision: parse_type_length(ty, &["TIMESTAMP"]).flatten(),
            with_timezone: normalized.contains("WITH TIME ZONE"),
        });
    }

    if normalized.starts_with("TIME") {
        return Some(DataType::Time {
            precision: parse_type_length(ty, &["TIME"]).flatten(),
            with_timezone: normalized.contains("WITH TIME ZONE"),
        });
    }

    None
}

fn parse_mysql_time_type(ty: &str) -> Option<DataType> {
    let normalized = normalize_type_name(ty);

    if normalized.starts_with("TIMESTAMP") {
        return Some(DataType::Timestamp {
            precision: parse_type_length(ty, &["TIMESTAMP"]).flatten(),
            with_timezone: false,
        });
    }

    if normalized.starts_with("TIME") {
        return Some(DataType::Time {
            precision: parse_type_length(ty, &["TIME"]).flatten(),
            with_timezone: false,
        });
    }

    None
}

fn parse_type_length(ty: &str, names: &[&str]) -> Option<Option<u32>> {
    let normalized = normalize_type_name(ty);

    for name in names {
        if normalized == *name {
            return Some(None);
        }
        let prefix = format!("{}(", name);
        let end = normalized.find(")");
        if normalized.starts_with(&prefix)
            && let Some(end) = end
        {
            let inner = &normalized[prefix.len()..end];
            return Some(inner.trim().parse().ok());
        }
    }

    None
}

fn parse_precision_and_scale(ty: &str, name: &str) -> Option<(Option<u32>, Option<u32>)> {
    let normalized = normalize_type_name(ty);

    if normalized == name {
        return Some((None, None));
    }

    let prefix = format!("{}(", name);
    if !normalized.starts_with(&prefix) || !normalized.ends_with(')') {
        return None;
    }

    let inner = &normalized[prefix.len()..normalized.len() - 1];
    let mut parts = inner.split(',').map(str::trim);
    let precision = parts.next().and_then(|part| part.parse().ok());
    let scale = parts.next().and_then(|part| part.parse().ok());
    Some((precision, scale))
}

fn parse_mysql_value_list(ty: &str, name: &str) -> Option<Vec<String>> {
    let normalized = normalize_type_name(ty);
    let prefix = format!("{}(", name);
    if !normalized.starts_with(&prefix) || !normalized.ends_with(')') {
        return None;
    }

    let start = ty.find('(')? + 1;
    let end = ty.rfind(')')?;
    let inner = &ty[start..end];
    parse_sql_string_list(inner)
}

fn parse_sql_string_list(values: &str) -> Option<Vec<String>> {
    let mut chars = values.chars().peekable();
    let mut parsed = Vec::new();

    loop {
        while matches!(chars.peek(), Some(c) if c.is_ascii_whitespace() || *c == ',') {
            chars.next();
        }

        if chars.peek().is_none() {
            break;
        }

        if chars.next()? != '\'' {
            return None;
        }

        let mut value = String::new();
        loop {
            match chars.next()? {
                '\'' => {
                    if matches!(chars.peek(), Some('\'')) {
                        chars.next();
                        value.push('\'');
                    } else {
                        break;
                    }
                }
                ch => value.push(ch),
            }
        }

        parsed.push(value);
    }

    Some(parsed)
}

fn parse_qualified_identifier(ty: &str) -> Option<(String, Option<String>)> {
    if ty.contains('(') || ty.contains(' ') || ty.is_empty() {
        return None;
    }

    let (schema, name) = match ty.rsplit_once('.') {
        Some((schema, name)) => (
            Some(unquote_identifier(schema.trim())),
            unquote_identifier(name.trim()),
        ),
        None => (None, unquote_identifier(ty)),
    };

    Some((name, schema))
}

fn unquote_identifier(value: &str) -> String {
    value.trim_matches('"').trim_matches('`').to_string()
}

fn normalize_type_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Default value for a column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum DefaultValue {
    /// Literal value
    Literal(String),
    /// SQL expression (e.g., CURRENT_TIMESTAMP)
    Sql(String),
    /// NULL
    Null,
    /// Sequence/auto-increment
    Sequence(String),
    /// Generated identity (PostgreSQL)
    Identity { always: bool },
}

impl std::fmt::Display for DefaultValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefaultValue::Null => write!(f, "null"),
            DefaultValue::Identity { always } => {
                write!(f, "{}", if *always { "ALWAYS" } else { "BY DEFAULT" })
            }
            DefaultValue::Sql(expression) => write!(f, "{}", expression),
            DefaultValue::Literal(val) => write!(f, "{}", val),
            DefaultValue::Sequence(val) => write!(f, "{}", val),
        }
    }
}

impl DefaultValue {
    /// Create a literal default value
    pub fn literal(value: impl Into<String>) -> Self {
        DefaultValue::Literal(value.into())
    }

    /// Create an expression default value
    pub fn expression(expr: impl Into<String>) -> Self {
        DefaultValue::Sql(expr.into())
    }

    /// Create a CURRENT_TIMESTAMP default
    pub fn current_timestamp() -> Self {
        DefaultValue::Sql("CURRENT_TIMESTAMP".to_string())
    }

    /// Create a now() default (PostgreSQL)
    pub fn now() -> Self {
        DefaultValue::Sql("now()".to_string())
    }

    /// Create a UUID generation default
    pub fn uuid_generate_v4() -> Self {
        DefaultValue::Sql("uuid_generate_v4()".to_string())
    }

    /// Create a uuidv7() default (PostgreSQL 18+)
    pub fn uuidv7() -> Self {
        DefaultValue::Sql("uuidv7()".to_string())
    }

    /// Create a `uuidv7()` default (PostgreSQL 18+)
    ///
    /// _Alias for `uuid_generate_v4()` in PostgreSQL 18+_
    pub fn uuidv4() -> Self {
        DefaultValue::Sql("uuidv7()".to_string())
    }

    /// Create a gen_random_uuid() default (PostgreSQL)
    pub fn gen_random_uuid() -> Self {
        DefaultValue::Sql("gen_random_uuid()".to_string())
    }
}

/// Generated column specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedColumn {
    /// SQL expression for the generated column
    pub expression: String,
    /// Whether the value is stored or virtual
    pub stored: bool,
}

impl std::fmt::Display for GeneratedColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stored {
            write!(f, "GENERATED ALWAYS AS ({}) STORED", self.expression)
        } else {
            write!(f, "GENERATED ALWAYS AS ({})", self.expression)
        }
    }
}

/// On conflict action for constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum OnConflict {
    #[default]
    Abort,
    Rollback,
    Fail,
    Ignore,
    Replace,
}

/// Referential action for foreign keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReferenceAction {
    #[default]
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl std::fmt::Display for ReferenceAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sql())
    }
}

impl ReferenceAction {
    pub fn to_sql(&self) -> &'static str {
        match self {
            ReferenceAction::NoAction => "NO ACTION",
            ReferenceAction::Restrict => "RESTRICT",
            ReferenceAction::Cascade => "CASCADE",
            ReferenceAction::SetNull => "SET NULL",
            ReferenceAction::SetDefault => "SET DEFAULT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_postgres_types_from_dialect_tuple() {
        assert_eq!(
            DataType::from((SqlDialect::Postgres, "VARCHAR(255)".to_string())),
            DataType::VarChar { length: Some(255) }
        );
        assert_eq!(
            DataType::from((SqlDialect::Postgres, "NUMERIC(10, 2)".to_string())),
            DataType::Numeric {
                precision: Some(10),
                scale: Some(2),
            }
        );
        assert_eq!(
            DataType::from((
                SqlDialect::Postgres,
                "TIMESTAMP(3) WITH TIME ZONE".to_string()
            )),
            DataType::Timestamp {
                precision: Some(3),
                with_timezone: true,
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Postgres, "INTEGER[]".to_string())),
            DataType::Array {
                element_type: Box::new(DataType::Integer),
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Postgres, "\"public\".\"status\"".to_string())),
            DataType::Custom {
                name: "status".to_string(),
                schema: Some("public".to_string()),
            }
        );
    }

    #[test]
    fn parses_mysql_types_from_dialect_tuple() {
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "INT AUTO_INCREMENT".to_string())),
            DataType::Serial
        );
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "TINYINT(1)".to_string())),
            DataType::Boolean
        );
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "MEDIUMINT UNSIGNED".to_string())),
            DataType::MediumInt { unsigned: true }
        );
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "ENUM('draft', 'published')".to_string())),
            DataType::Enum_ {
                values: vec!["draft".to_string(), "published".to_string()],
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "VARBINARY(255)".to_string())),
            DataType::VarBinary { length: Some(255) }
        );
    }

    #[test]
    fn parses_sqlite_types_from_dialect_tuple() {
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "INTEGER".to_string())),
            DataType::SqliteInteger
        );
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "VARCHAR(255)".to_string())),
            DataType::VarChar { length: Some(255) }
        );
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "NUMERIC(10, 2)".to_string())),
            DataType::Numeric {
                precision: Some(10),
                scale: Some(2),
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "BOOLEAN".to_string())),
            DataType::Boolean
        );
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "BLOB".to_string())),
            DataType::SqliteBlob
        );
    }

    #[test]
    fn parses_types_from_dialect_and_str_tuple() {
        assert_eq!(
            DataType::from((SqlDialect::Postgres, "INTEGER[][]")),
            DataType::Array {
                element_type: Box::new(DataType::Array {
                    element_type: Box::new(DataType::Integer),
                }),
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Mysql, "SET('draft', 'published')")),
            DataType::Set {
                values: vec!["draft".to_string(), "published".to_string()],
            }
        );
        assert_eq!(
            DataType::from((SqlDialect::Sqlite, "jsonb")),
            DataType::JsonB
        );
    }

    #[test]
    fn parses_types_from_borrowed_dialect_and_str_tuple() {
        let postgres = SqlDialect::Postgres;
        let mysql = SqlDialect::Mysql;
        let sqlite = SqlDialect::Sqlite;

        assert_eq!(
            DataType::from((&postgres, "TIME(6) WITH TIME ZONE")),
            DataType::Time {
                precision: Some(6),
                with_timezone: true,
            }
        );
        assert_eq!(
            DataType::from((&mysql, "DECIMAL(8, 3)")),
            DataType::Decimal {
                precision: Some(8),
                scale: Some(3),
            }
        );
        assert_eq!(
            DataType::from((&sqlite, "double precision")),
            DataType::Real
        );
    }

    #[test]
    fn displays_default_identity_values() {
        assert_eq!(
            DefaultValue::Identity { always: true }.to_string(),
            "ALWAYS"
        );
        assert_eq!(
            DefaultValue::Identity { always: false }.to_string(),
            "BY DEFAULT"
        );
    }
}
