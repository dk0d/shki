//! SQL data types for different dialects

use serde::{Deserialize, Serialize};

/// Enum type definition (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumType {
    pub name: String,
    pub schema: Option<String>,
    pub values: Vec<String>,
    /// Description/comment for the enum (used for SQL comments and Rust doc comments)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl EnumType {
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
