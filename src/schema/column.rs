//! Column definitions

use serde::{Deserialize, Serialize};

use super::types::{DataType, DefaultValue, GeneratedColumn};

/// A database column definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Column name
    pub name: String,

    /// Data type
    pub data_type: DataType,

    /// Whether the column is nullable
    #[serde(default = "default_true")]
    pub nullable: bool,

    /// Default value
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,

    /// Whether this column is a primary key
    #[serde(default)]
    pub primary_key: bool,

    /// Whether this column has a unique constraint
    #[serde(default)]
    pub unique: bool,

    /// Generated column expression
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<GeneratedColumn>,

    /// Column comment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Collation (for string types)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,

    /// Identity column specification (PostgreSQL)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentitySpec>,

    /// Foreign key reference (for inline FK definitions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<ColumnReference>,
}

fn default_true() -> bool {
    true
}

/// Identity column specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentitySpec {
    /// ALWAYS or BY DEFAULT
    pub always: bool,
    /// Sequence options
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_options: Option<SequenceOptions>,
}

/// Sequence options for identity columns
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SequenceOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub increment: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<i64>,
    #[serde(default)]
    pub cycle: bool,
}

/// Column-level foreign key reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnReference {
    pub table: String,
    pub column: String,
    #[serde(default)]
    pub on_delete: super::types::ReferenceAction,
    #[serde(default)]
    pub on_update: super::types::ReferenceAction,
}

impl Column {
    /// Create a new column with the given name and type
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: true,
            default: None,
            primary_key: false,
            unique: false,
            generated: None,
            comment: None,
            collation: None,
            identity: None,
            references: None,
        }
    }

    /// Set the column as not nullable
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    /// Set the column as nullable
    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// Set the column as a primary key
    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.nullable = false;
        self
    }

    /// Set the column as unique
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Set a default value
    pub fn default(mut self, value: DefaultValue) -> Self {
        self.default = Some(value);
        self
    }

    /// Set a default literal value
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(DefaultValue::Literal(value.into()));
        self
    }

    /// Set default to CURRENT_TIMESTAMP / now()
    pub fn default_now(mut self) -> Self {
        self.default = Some(DefaultValue::now());
        self
    }

    /// Set as a generated column
    pub fn generated_as(mut self, expression: impl Into<String>, stored: bool) -> Self {
        self.generated = Some(GeneratedColumn {
            expression: expression.into(),
            stored,
        });
        self
    }

    /// Set a comment on the column
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Set the collation
    pub fn collate(mut self, collation: impl Into<String>) -> Self {
        self.collation = Some(collation.into());
        self
    }

    /// Set as an identity column (PostgreSQL)
    pub fn identity(mut self, always: bool) -> Self {
        self.identity = Some(IdentitySpec {
            always,
            sequence_options: None,
        });
        self
    }

    /// Set a foreign key reference
    pub fn references_column(
        mut self,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        self.references = Some(ColumnReference {
            table: table.into(),
            column: column.into(),
            on_delete: super::types::ReferenceAction::NoAction,
            on_update: super::types::ReferenceAction::NoAction,
        });
        self
    }

    /// Set a foreign key reference with ON DELETE action
    pub fn references_with_delete(
        mut self,
        table: impl Into<String>,
        column: impl Into<String>,
        on_delete: super::types::ReferenceAction,
    ) -> Self {
        self.references = Some(ColumnReference {
            table: table.into(),
            column: column.into(),
            on_delete,
            on_update: super::types::ReferenceAction::NoAction,
        });
        self
    }
}

/// Builder for creating columns with a fluent API
///
/// # Example
///
/// ```rust
/// use shki::schema::ColumnBuilder;
/// use shki::schema::types::DataType;
///
/// // Using type-specific constructors
/// let id = ColumnBuilder::serial("id").primary_key();
/// let name = ColumnBuilder::text("name").not_null();
/// let email = ColumnBuilder::text("email").not_null().unique();
/// let created_at = ColumnBuilder::timestamptz("created_at").default_now();
///
/// // Using generic constructor
/// let custom = ColumnBuilder::new("custom", DataType::Integer).not_null();
/// ```
pub struct ColumnBuilder {
    column: Column,
}

impl ColumnBuilder {
    /// Create a new column builder with explicit type
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            column: Column::new(name, data_type),
        }
    }

    // ==================== Type-specific constructors ====================

    /// Create a SERIAL column (auto-incrementing integer)
    pub fn serial(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Serial)
    }

    /// Create a BIGSERIAL column (auto-incrementing big integer)
    pub fn bigserial(name: impl Into<String>) -> Self {
        Self::new(name, DataType::BigSerial)
    }

    /// Create a SMALLSERIAL column
    pub fn smallserial(name: impl Into<String>) -> Self {
        Self::new(name, DataType::SmallSerial)
    }

    /// Create an INTEGER column
    pub fn integer(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Integer)
    }

    /// Create a BIGINT column
    pub fn bigint(name: impl Into<String>) -> Self {
        Self::new(name, DataType::BigInt)
    }

    /// Create a SMALLINT column
    pub fn smallint(name: impl Into<String>) -> Self {
        Self::new(name, DataType::SmallInt)
    }

    /// Create a TEXT column
    pub fn text(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Text)
    }

    /// Create a VARCHAR column
    pub fn varchar(name: impl Into<String>, length: Option<u32>) -> Self {
        Self::new(name, DataType::VarChar { length })
    }

    /// Create a CHAR column
    pub fn char(name: impl Into<String>, length: Option<u32>) -> Self {
        Self::new(name, DataType::Char { length })
    }

    /// Create a BOOLEAN column
    pub fn boolean(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Boolean)
    }

    /// Create a TIMESTAMP column (without timezone)
    pub fn timestamp(name: impl Into<String>) -> Self {
        Self::new(
            name,
            DataType::Timestamp {
                precision: None,
                with_timezone: false,
            },
        )
    }

    /// Create a TIMESTAMPTZ column (with timezone)
    pub fn timestamptz(name: impl Into<String>) -> Self {
        Self::new(
            name,
            DataType::Timestamp {
                precision: None,
                with_timezone: true,
            },
        )
    }

    /// Create a DATE column
    pub fn date(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Date)
    }

    /// Create a TIME column
    pub fn time(name: impl Into<String>) -> Self {
        Self::new(
            name,
            DataType::Time {
                precision: None,
                with_timezone: false,
            },
        )
    }

    /// Create a UUID column
    pub fn uuid(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Uuid)
    }

    /// Create a JSON column
    pub fn json(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Json)
    }

    /// Create a JSONB column (PostgreSQL binary JSON)
    pub fn jsonb(name: impl Into<String>) -> Self {
        Self::new(name, DataType::JsonB)
    }

    /// Create a NUMERIC/DECIMAL column
    pub fn numeric(name: impl Into<String>, precision: Option<u32>, scale: Option<u32>) -> Self {
        Self::new(name, DataType::Numeric { precision, scale })
    }

    /// Create a REAL (float4) column
    pub fn real(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Real)
    }

    /// Create a DOUBLE PRECISION (float8) column
    pub fn double_precision(name: impl Into<String>) -> Self {
        Self::new(name, DataType::DoublePrecision)
    }

    /// Create a BYTEA (binary) column
    pub fn bytea(name: impl Into<String>) -> Self {
        Self::new(name, DataType::ByteA)
    }

    /// Create an INET column (PostgreSQL IP address)
    pub fn inet(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Inet)
    }

    /// Create a CIDR column (PostgreSQL network address)
    pub fn cidr(name: impl Into<String>) -> Self {
        Self::new(name, DataType::Cidr)
    }

    /// Create an enum column
    pub fn enum_type(name: impl Into<String>, enum_name: impl Into<String>) -> Self {
        Self::new(
            name,
            DataType::Enum {
                name: enum_name.into(),
                schema: None,
            },
        )
    }

    /// Create an array column
    pub fn array(name: impl Into<String>, element_type: DataType) -> Self {
        Self::new(
            name,
            DataType::Array {
                element_type: Box::new(element_type),
            },
        )
    }

    // ==================== Column modifiers ====================

    /// Set the column as not nullable
    pub fn not_null(mut self) -> Self {
        self.column = self.column.not_null();
        self
    }

    /// Set the column as nullable
    pub fn nullable(mut self) -> Self {
        self.column = self.column.nullable();
        self
    }

    /// Set the column as a primary key
    pub fn primary_key(mut self) -> Self {
        self.column = self.column.primary_key();
        self
    }

    /// Set the column as unique
    pub fn unique(mut self) -> Self {
        self.column = self.column.unique();
        self
    }

    /// Set a default value
    pub fn default(mut self, value: DefaultValue) -> Self {
        self.column = self.column.default(value);
        self
    }

    /// Set a default literal value
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.column = self.column.default_value(value);
        self
    }

    /// Set default to now()
    pub fn default_now(mut self) -> Self {
        self.column = self.column.default_now();
        self
    }

    /// Set as a generated column
    pub fn generated_as(mut self, expression: impl Into<String>, stored: bool) -> Self {
        self.column = self.column.generated_as(expression, stored);
        self
    }

    /// Set a comment
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.column = self.column.comment(comment);
        self
    }

    /// Set a description for the column (alias for comment)
    ///
    /// The description is used for SQL comments and Rust doc comments in generated code.
    pub fn description(self, description: impl Into<String>) -> Self {
        self.comment(description)
    }

    /// Set the collation
    pub fn collate(mut self, collation: impl Into<String>) -> Self {
        self.column = self.column.collate(collation);
        self
    }

    /// Set as an identity column (PostgreSQL)
    pub fn identity(mut self, always: bool) -> Self {
        self.column = self.column.identity(always);
        self
    }

    /// Set a foreign key reference
    pub fn references(mut self, table: impl Into<String>, column: impl Into<String>) -> Self {
        self.column = self.column.references_column(table, column);
        self
    }

    /// Set a foreign key reference with ON DELETE action
    pub fn references_on_delete(
        mut self,
        table: impl Into<String>,
        column: impl Into<String>,
        on_delete: super::types::ReferenceAction,
    ) -> Self {
        self.column = self.column.references_with_delete(table, column, on_delete);
        self
    }

    /// Build the column
    pub fn build(self) -> Column {
        self.column
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::types::{DataType, DefaultValue, ReferenceAction};

    // ==================== Column Tests ====================

    #[test]
    fn test_column_new() {
        let col = Column::new("name", DataType::Text);
        assert_eq!(col.name, "name");
        assert!(matches!(col.data_type, DataType::Text));
        assert!(col.nullable); // Default is nullable
        assert!(col.default.is_none());
        assert!(!col.primary_key);
        assert!(!col.unique);
        assert!(col.comment.is_none());
    }

    #[test]
    fn test_column_not_null() {
        let col = Column::new("name", DataType::Text).not_null();
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_nullable() {
        let col = Column::new("name", DataType::Text).not_null().nullable();
        assert!(col.nullable);
    }

    #[test]
    fn test_column_primary_key() {
        let col = Column::new("id", DataType::Serial).primary_key();
        assert!(col.primary_key);
        assert!(!col.nullable); // PK should set not null
    }

    #[test]
    fn test_column_unique() {
        let col = Column::new("email", DataType::Text).unique();
        assert!(col.unique);
    }

    #[test]
    fn test_column_default() {
        let col = Column::new("status", DataType::Text)
            .default(DefaultValue::Literal("active".to_string()));
        assert!(matches!(col.default, Some(DefaultValue::Literal(s)) if s == "active"));
    }

    #[test]
    fn test_column_default_value() {
        let col = Column::new("status", DataType::Text).default_value("active");
        assert!(matches!(col.default, Some(DefaultValue::Literal(s)) if s == "active"));
    }

    #[test]
    fn test_column_default_now() {
        let col = Column::new(
            "created_at",
            DataType::Timestamp {
                precision: None,
                with_timezone: true,
            },
        )
        .default_now();
        assert!(matches!(col.default, Some(DefaultValue::Sql(s)) if s == "now()"));
    }

    #[test]
    fn test_column_generated() {
        let col = Column::new("full_name", DataType::Text)
            .generated_as("first_name || ' ' || last_name", true);

        assert!(col.generated.is_some());
        let generated = col.generated.expect("generated spec not set");
        assert_eq!(generated.expression, "first_name || ' ' || last_name");
        assert!(generated.stored);
    }

    #[test]
    fn test_column_comment() {
        let col = Column::new("email", DataType::Text).comment("User's email address");
        assert_eq!(col.comment, Some("User's email address".to_string()));
    }

    #[test]
    fn test_column_collate() {
        let col = Column::new("name", DataType::Text).collate("en_US.utf8");
        assert_eq!(col.collation, Some("en_US.utf8".to_string()));
    }

    #[test]
    fn test_column_identity() {
        let col = Column::new("id", DataType::Integer).identity(true);
        assert!(col.identity.is_some());
        assert!(col.identity.expect("identity spec not set").always);
    }

    #[test]
    fn test_column_references() {
        let col = Column::new("user_id", DataType::Integer).references_column("users", "id");

        assert!(col.references.is_some());
        let refs = col.references.expect("column reference not set");
        assert_eq!(refs.table, "users");
        assert_eq!(refs.column, "id");
    }

    #[test]
    fn test_column_references_with_delete() {
        let col = Column::new("user_id", DataType::Integer).references_with_delete(
            "users",
            "id",
            ReferenceAction::Cascade,
        );

        let refs = col.references.expect("column reference not set");
        assert_eq!(refs.on_delete, ReferenceAction::Cascade);
    }

    // ==================== ColumnBuilder Type Constructors ====================

    #[test]
    fn test_column_builder_serial() {
        let col = ColumnBuilder::serial("id").build();
        assert_eq!(col.name, "id");
        assert!(matches!(col.data_type, DataType::Serial));
    }

    #[test]
    fn test_column_builder_bigserial() {
        let col = ColumnBuilder::bigserial("id").build();
        assert!(matches!(col.data_type, DataType::BigSerial));
    }

    #[test]
    fn test_column_builder_smallserial() {
        let col = ColumnBuilder::smallserial("id").build();
        assert!(matches!(col.data_type, DataType::SmallSerial));
    }

    #[test]
    fn test_column_builder_integer() {
        let col = ColumnBuilder::integer("count").build();
        assert!(matches!(col.data_type, DataType::Integer));
    }

    #[test]
    fn test_column_builder_bigint() {
        let col = ColumnBuilder::bigint("big_count").build();
        assert!(matches!(col.data_type, DataType::BigInt));
    }

    #[test]
    fn test_column_builder_smallint() {
        let col = ColumnBuilder::smallint("small_count").build();
        assert!(matches!(col.data_type, DataType::SmallInt));
    }

    #[test]
    fn test_column_builder_text() {
        let col = ColumnBuilder::text("content").build();
        assert!(matches!(col.data_type, DataType::Text));
    }

    #[test]
    fn test_column_builder_varchar() {
        let col = ColumnBuilder::varchar("name", Some(255)).build();
        assert!(matches!(
            col.data_type,
            DataType::VarChar { length: Some(255) }
        ));
    }

    #[test]
    fn test_column_builder_varchar_no_length() {
        let col = ColumnBuilder::varchar("name", None).build();
        assert!(matches!(col.data_type, DataType::VarChar { length: None }));
    }

    #[test]
    fn test_column_builder_char() {
        let col = ColumnBuilder::char("code", Some(3)).build();
        assert!(matches!(col.data_type, DataType::Char { length: Some(3) }));
    }

    #[test]
    fn test_column_builder_boolean() {
        let col = ColumnBuilder::boolean("is_active").build();
        assert!(matches!(col.data_type, DataType::Boolean));
    }

    #[test]
    fn test_column_builder_timestamp() {
        let col = ColumnBuilder::timestamp("created_at").build();
        assert!(matches!(
            col.data_type,
            DataType::Timestamp {
                with_timezone: false,
                ..
            }
        ));
    }

    #[test]
    fn test_column_builder_timestamptz() {
        let col = ColumnBuilder::timestamptz("created_at").build();
        assert!(matches!(
            col.data_type,
            DataType::Timestamp {
                with_timezone: true,
                ..
            }
        ));
    }

    #[test]
    fn test_column_builder_date() {
        let col = ColumnBuilder::date("birth_date").build();
        assert!(matches!(col.data_type, DataType::Date));
    }

    #[test]
    fn test_column_builder_time() {
        let col = ColumnBuilder::time("start_time").build();
        assert!(matches!(
            col.data_type,
            DataType::Time {
                with_timezone: false,
                ..
            }
        ));
    }

    #[test]
    fn test_column_builder_uuid() {
        let col = ColumnBuilder::uuid("id").build();
        assert!(matches!(col.data_type, DataType::Uuid));
    }

    #[test]
    fn test_column_builder_json() {
        let col = ColumnBuilder::json("data").build();
        assert!(matches!(col.data_type, DataType::Json));
    }

    #[test]
    fn test_column_builder_jsonb() {
        let col = ColumnBuilder::jsonb("data").build();
        assert!(matches!(col.data_type, DataType::JsonB));
    }

    #[test]
    fn test_column_builder_numeric() {
        let col = ColumnBuilder::numeric("price", Some(10), Some(2)).build();
        assert!(matches!(
            col.data_type,
            DataType::Numeric {
                precision: Some(10),
                scale: Some(2)
            }
        ));
    }

    #[test]
    fn test_column_builder_real() {
        let col = ColumnBuilder::real("score").build();
        assert!(matches!(col.data_type, DataType::Real));
    }

    #[test]
    fn test_column_builder_double_precision() {
        let col = ColumnBuilder::double_precision("amount").build();
        assert!(matches!(col.data_type, DataType::DoublePrecision));
    }

    #[test]
    fn test_column_builder_bytea() {
        let col = ColumnBuilder::bytea("data").build();
        assert!(matches!(col.data_type, DataType::ByteA));
    }

    #[test]
    fn test_column_builder_inet() {
        let col = ColumnBuilder::inet("ip_address").build();
        assert!(matches!(col.data_type, DataType::Inet));
    }

    #[test]
    fn test_column_builder_cidr() {
        let col = ColumnBuilder::cidr("network").build();
        assert!(matches!(col.data_type, DataType::Cidr));
    }

    #[test]
    fn test_column_builder_enum_type() {
        let col = ColumnBuilder::enum_type("status", "post_status").build();
        assert!(matches!(col.data_type, DataType::Enum { name, .. } if name == "post_status"));
    }

    #[test]
    fn test_column_builder_array() {
        let col = ColumnBuilder::array("tags", DataType::Text).build();
        assert!(matches!(col.data_type, DataType::Array { .. }));
    }

    // ==================== ColumnBuilder Modifiers ====================

    #[test]
    fn test_column_builder_not_null() {
        let col = ColumnBuilder::text("name").not_null().build();
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_builder_nullable() {
        let col = ColumnBuilder::text("name").not_null().nullable().build();
        assert!(col.nullable);
    }

    #[test]
    fn test_column_builder_primary_key() {
        let col = ColumnBuilder::serial("id").primary_key().build();
        assert!(col.primary_key);
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_builder_unique() {
        let col = ColumnBuilder::text("email").unique().build();
        assert!(col.unique);
    }

    #[test]
    fn test_column_builder_default() {
        let col = ColumnBuilder::text("status")
            .default(DefaultValue::Literal("pending".to_string()))
            .build();
        assert!(col.default.is_some());
    }

    #[test]
    fn test_column_builder_default_value() {
        let col = ColumnBuilder::text("status")
            .default_value("active")
            .build();
        assert!(matches!(col.default, Some(DefaultValue::Literal(s)) if s == "active"));
    }

    #[test]
    fn test_column_builder_default_now() {
        let col = ColumnBuilder::timestamptz("created_at")
            .default_now()
            .build();
        assert!(matches!(col.default, Some(DefaultValue::Sql(s)) if s == "now()"));
    }

    #[test]
    fn test_column_builder_generated_as() {
        let col = ColumnBuilder::integer("age")
            .generated_as(
                "EXTRACT(YEAR FROM CURRENT_DATE) - EXTRACT(YEAR FROM birth_date)",
                true,
            )
            .build();
        assert!(col.generated.is_some());
    }

    #[test]
    fn test_column_builder_comment() {
        let col = ColumnBuilder::text("email")
            .comment("User's primary email")
            .build();
        assert_eq!(col.comment, Some("User's primary email".to_string()));
    }

    #[test]
    fn test_column_builder_description() {
        let col = ColumnBuilder::text("email")
            .description("User's primary email")
            .build();
        assert_eq!(col.comment, Some("User's primary email".to_string()));
    }

    #[test]
    fn test_column_builder_description_multiline() {
        let col = ColumnBuilder::text("email")
            .description("User's primary email.\n\nMust be unique.")
            .build();
        assert_eq!(
            col.comment,
            Some("User's primary email.\n\nMust be unique.".to_string())
        );
    }

    #[test]
    fn test_column_builder_collate() {
        let col = ColumnBuilder::text("name").collate("en_US").build();
        assert_eq!(col.collation, Some("en_US".to_string()));
    }

    #[test]
    fn test_column_builder_identity() {
        let col = ColumnBuilder::integer("id").identity(true).build();
        assert!(col.identity.is_some());
    }

    #[test]
    fn test_column_builder_references() {
        let col = ColumnBuilder::integer("user_id")
            .references("users", "id")
            .build();

        assert!(col.references.is_some());
        let refs = col.references.expect("column reference not set");
        assert_eq!(refs.table, "users");
        assert_eq!(refs.column, "id");
    }

    #[test]
    fn test_column_builder_references_on_delete() {
        let col = ColumnBuilder::integer("user_id")
            .references_on_delete("users", "id", ReferenceAction::Cascade)
            .build();

        let refs = col.references.expect("column reference not set");
        assert_eq!(refs.on_delete, ReferenceAction::Cascade);
    }

    // ==================== ColumnBuilder Chaining ====================

    #[test]
    fn test_column_builder_full_chain() {
        let col = ColumnBuilder::text("email")
            .not_null()
            .unique()
            .description("User's email address")
            .collate("en_US.utf8")
            .build();

        assert_eq!(col.name, "email");
        assert!(!col.nullable);
        assert!(col.unique);
        assert_eq!(col.comment, Some("User's email address".to_string()));
        assert_eq!(col.collation, Some("en_US.utf8".to_string()));
    }

    #[test]
    fn test_column_builder_complex_example() {
        let col = ColumnBuilder::integer("author_id")
            .not_null()
            .references_on_delete("users", "id", ReferenceAction::Cascade)
            .description("Foreign key to users table")
            .build();

        assert_eq!(col.name, "author_id");
        assert!(!col.nullable);
        assert!(col.references.is_some());
        assert_eq!(col.comment, Some("Foreign key to users table".to_string()));
    }

    #[test]
    fn test_column_builder_into_column() {
        let builder = ColumnBuilder::text("name").not_null();
        let col: Column = builder.into();
        assert_eq!(col.name, "name");
        assert!(!col.nullable);
    }

    // ==================== Identity Tests ====================

    #[test]
    fn test_identity_spec() {
        let spec = IdentitySpec {
            always: true,
            sequence_options: None,
        };
        assert!(spec.always);
    }

    #[test]
    fn test_identity_spec_with_options() {
        let spec = IdentitySpec {
            always: false,
            sequence_options: Some(SequenceOptions {
                start: Some(100),
                increment: Some(10),
                ..Default::default()
            }),
        };
        assert!(!spec.always);
        assert_eq!(
            spec.sequence_options
                .as_ref()
                .expect("sequence options not set")
                .start,
            Some(100)
        );
    }

    // ==================== ColumnReference Tests ====================

    #[test]
    fn test_column_reference() {
        let refs = ColumnReference {
            table: "users".to_string(),
            column: "id".to_string(),
            on_delete: ReferenceAction::Cascade,
            on_update: ReferenceAction::NoAction,
        };
        assert_eq!(refs.table, "users");
        assert_eq!(refs.column, "id");
    }
}
