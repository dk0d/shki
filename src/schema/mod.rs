pub mod column;
pub use column::*;
pub mod types;
pub use types::*;
pub mod table;
pub use table::*;
pub mod constraint;
pub use constraint::*;
pub mod index;
pub use index::*;
pub mod sequence;
pub use sequence::*;
pub mod views;
pub use views::*;
pub mod builders;
pub use builders::*;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Database dialect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SchemaDialect {
    #[default]
    Postgres,
    Mysql,
    Sqlite,
}

impl std::fmt::Display for SchemaDialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaDialect::Postgres => write!(f, "postgresql"),
            SchemaDialect::Mysql => write!(f, "mysql"),
            SchemaDialect::Sqlite => write!(f, "sqlite"),
        }
    }
}

/// A complete database schema containing all objects
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Schema {
    /// Schema name (e.g., "public" for PostgreSQL)
    pub name: String,

    /// Database dialect
    pub dialect: SchemaDialect,

    /// Tables in the schema
    #[serde(default)]
    pub tables: IndexMap<String, Table>,

    /// Enums (PostgreSQL)
    #[serde(default)]
    pub enums: IndexMap<String, EnumType>,

    /// Sequences
    #[serde(default)]
    pub sequences: IndexMap<String, Sequence>,

    /// Views
    #[serde(default)]
    pub views: IndexMap<String, View>,

    /// Extensions (PostgreSQL)
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl Schema {
    /// Create a new schema with the given name and dialect
    pub fn new(name: impl Into<String>, dialect: SchemaDialect) -> Self {
        Self {
            name: name.into(),
            dialect,
            tables: IndexMap::new(),
            enums: IndexMap::new(),
            sequences: IndexMap::new(),
            views: IndexMap::new(),
            extensions: Vec::new(),
        }
    }

    /// Create a new PostgreSQL schema
    pub fn postgres(name: impl Into<String>) -> Self {
        Self::new(name, SchemaDialect::Postgres)
    }

    /// Create a new MySQL schema
    pub fn mysql(name: impl Into<String>) -> Self {
        Self::new(name, SchemaDialect::Mysql)
    }

    /// Create a new SQLite schema
    pub fn sqlite() -> Self {
        Self::new("main", SchemaDialect::Sqlite)
    }

    /// Add a table to the schema
    pub fn add_table(&mut self, table: Table) -> &mut Self {
        self.tables.insert(table.name.clone(), table);
        self
    }

    /// Add an enum to the schema
    ///
    /// The schema name is automatically set to this schema's name if not already set.
    pub fn enum_type(&mut self, enum_type: impl Into<EnumType>) -> &mut Self {
        let mut enum_type = enum_type.into();
        // Set the schema if not already set
        if enum_type.schema.is_none() {
            enum_type.schema = Some(self.name.clone());
        }
        self.enums.insert(enum_type.name.clone(), enum_type);
        self
    }

    /// Add a sequence to the schema
    pub fn sequence(&mut self, sequence: Sequence) -> &mut Self {
        self.sequences.insert(sequence.name.clone(), sequence);
        self
    }

    /// Add a view to the schema
    pub fn view(&mut self, view: View) -> &mut Self {
        self.views.insert(view.name.clone(), view);
        self
    }

    /// Add an extension (PostgreSQL)
    pub fn extension(&mut self, extension: impl Into<String>) -> &mut Self {
        self.extensions.push(extension.into());
        self
    }

    /// Get a table by name
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Get a mutable table by name
    pub fn get_table_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.get_mut(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== EnumType Tests ====================

    #[test]
    fn test_enum_type_new() {
        let enum_type = EnumType::new("status");
        assert_eq!(enum_type.name, "status");
        assert!(enum_type.schema.is_none());
        assert!(enum_type.values.is_empty());
        assert!(enum_type.description.is_none());
    }

    #[test]
    fn test_enum_type_with_values() {
        let enum_type = EnumType::with_values("status", vec!["draft", "published"]);
        assert_eq!(enum_type.name, "status");
        assert_eq!(enum_type.values, vec!["draft", "published"]);
        assert!(enum_type.description.is_none());
    }

    #[test]
    fn test_enum_type_add_value() {
        let mut enum_type = EnumType::new("status");
        enum_type.add_value("draft");
        enum_type.add_value("published");
        assert_eq!(enum_type.values, vec!["draft", "published"]);
    }

    #[test]
    fn test_enum_type_in_schema() {
        let enum_type = EnumType::new("status").in_schema("custom");
        assert_eq!(enum_type.schema, Some("custom".to_string()));
    }

    // ==================== EnumBuilder Tests ====================

    #[test]
    fn test_enum_builder_new() {
        let builder = EnumBuilder::new("post_status");
        let enum_type = builder.build();
        assert_eq!(enum_type.name, "post_status");
        assert!(enum_type.values.is_empty());
        assert!(enum_type.description.is_none());
    }

    #[test]
    fn test_enum_builder_single_value() {
        let enum_type = EnumBuilder::new("status").value("active").build();
        assert_eq!(enum_type.values, vec!["active"]);
    }

    #[test]
    fn test_enum_builder_multiple_values_chained() {
        let enum_type = EnumBuilder::new("status")
            .value("draft")
            .value("pending")
            .value("published")
            .value("archived")
            .build();
        assert_eq!(
            enum_type.values,
            vec!["draft", "pending", "published", "archived"]
        );
    }

    #[test]
    fn test_enum_builder_values_batch() {
        let enum_type = EnumBuilder::new("role")
            .values(["admin", "moderator", "user", "guest"])
            .build();
        assert_eq!(
            enum_type.values,
            vec!["admin", "moderator", "user", "guest"]
        );
    }

    #[test]
    fn test_enum_builder_values_mixed() {
        let enum_type = EnumBuilder::new("status")
            .value("draft")
            .values(["pending", "published"])
            .value("archived")
            .build();
        assert_eq!(
            enum_type.values,
            vec!["draft", "pending", "published", "archived"]
        );
    }

    #[test]
    fn test_enum_builder_description() {
        let enum_type = EnumBuilder::new("post_status")
            .description("Status of a blog post")
            .value("draft")
            .build();
        assert_eq!(
            enum_type.description,
            Some("Status of a blog post".to_string())
        );
    }

    #[test]
    fn test_enum_builder_description_multiline() {
        let enum_type = EnumBuilder::new("status")
            .description("Status of an item.\n\nCan be one of several states.")
            .build();
        assert_eq!(
            enum_type.description,
            Some("Status of an item.\n\nCan be one of several states.".to_string())
        );
    }

    #[test]
    fn test_enum_builder_description_with_special_chars() {
        let enum_type = EnumBuilder::new("status")
            .description("Status with 'quotes' and \"double quotes\"")
            .build();
        assert_eq!(
            enum_type.description,
            Some("Status with 'quotes' and \"double quotes\"".to_string())
        );
    }

    #[test]
    fn test_enum_builder_full_fluent_api() {
        let enum_type = EnumBuilder::new("user_role")
            .description("User permission levels")
            .value("admin")
            .value("editor")
            .values(["viewer", "guest"])
            .build();

        assert_eq!(enum_type.name, "user_role");
        assert_eq!(
            enum_type.description,
            Some("User permission levels".to_string())
        );
        assert_eq!(enum_type.values, vec!["admin", "editor", "viewer", "guest"]);
    }

    #[test]
    fn test_enum_builder_into_enum_type() {
        let builder = EnumBuilder::new("status").value("active");
        let enum_type: EnumType = builder.into();
        assert_eq!(enum_type.name, "status");
        assert_eq!(enum_type.values, vec!["active"]);
    }

    #[test]
    fn test_enum_builder_empty_values() {
        let enum_type = EnumBuilder::new("empty_enum").build();
        assert!(enum_type.values.is_empty());
    }

    #[test]
    fn test_enum_builder_with_string_owned() {
        let name = String::from("dynamic_enum");
        let value = String::from("dynamic_value");
        let enum_type = EnumBuilder::new(name).value(value).build();
        assert_eq!(enum_type.name, "dynamic_enum");
        assert_eq!(enum_type.values, vec!["dynamic_value"]);
    }

    // ==================== Schema Tests ====================

    #[test]
    fn test_schema_new() {
        let schema = Schema::new("public", SchemaDialect::Postgres);
        assert_eq!(schema.name, "public");
        assert_eq!(schema.dialect, SchemaDialect::Postgres);
        assert!(schema.tables.is_empty());
        assert!(schema.enums.is_empty());
    }

    #[test]
    fn test_schema_postgres() {
        let schema = Schema::postgres("myschema");
        assert_eq!(schema.name, "myschema");
        assert_eq!(schema.dialect, SchemaDialect::Postgres);
    }

    #[test]
    fn test_schema_mysql() {
        let schema = Schema::mysql("mydb");
        assert_eq!(schema.name, "mydb");
        assert_eq!(schema.dialect, SchemaDialect::Mysql);
    }

    #[test]
    fn test_schema_sqlite() {
        let schema = Schema::sqlite();
        assert_eq!(schema.name, "main");
        assert_eq!(schema.dialect, SchemaDialect::Sqlite);
    }

    #[test]
    fn test_schema_add_enum() {
        let mut schema = Schema::postgres("public");
        schema.enum_type(EnumBuilder::new("status").value("active"));

        assert_eq!(schema.enums.len(), 1);
        assert!(schema.enums.contains_key("status"));

        let enum_type = schema.enums.get("status").unwrap();
        assert_eq!(enum_type.schema, Some("public".to_string()));
    }

    #[test]
    fn test_schema_add_enum_preserves_existing_schema() {
        let mut schema = Schema::postgres("public");
        let custom_enum = EnumType::new("status").in_schema("custom");
        schema.enum_type(custom_enum);

        let enum_type = schema.enums.get("status").unwrap();
        assert_eq!(enum_type.schema, Some("custom".to_string()));
    }

    #[test]
    fn test_schema_add_table() {
        let mut schema = Schema::postgres("public");
        let table = Table::new("users");
        schema.add_table(table);

        assert_eq!(schema.tables.len(), 1);
        assert!(schema.tables.contains_key("users"));
    }

    #[test]
    fn test_schema_add_extension() {
        let mut schema = Schema::postgres("public");
        schema.extension("uuid-ossp");
        schema.extension("pgcrypto");

        assert_eq!(schema.extensions, vec!["uuid-ossp", "pgcrypto"]);
    }

    #[test]
    fn test_schema_table_lookup() {
        let mut schema = Schema::postgres("public");
        let table = Table::new("users");
        schema.add_table(table);

        assert!(schema.table("users").is_some());
        assert!(schema.table("nonexistent").is_none());
    }

    #[test]
    fn test_schema_table_mut() {
        let mut schema = Schema::postgres("public");
        let table = Table::new("users");
        schema.add_table(table);

        let table_mut = schema.get_table_mut("users").unwrap();
        table_mut.comment = Some("Modified".to_string());

        assert_eq!(
            schema.table("users").unwrap().comment,
            Some("Modified".to_string())
        );
    }

    // ==================== SchemaBuilder Tests ====================

    #[test]
    fn test_schema_builder_new() {
        let schema = SchemaBuilder::new("public", SchemaDialect::Postgres).build();
        assert_eq!(schema.name, "public");
        assert_eq!(schema.dialect, SchemaDialect::Postgres);
    }

    #[test]
    fn test_schema_builder_extension() {
        let schema = SchemaBuilder::new("public", SchemaDialect::Postgres)
            .extension("uuid-ossp")
            .extension("pgcrypto")
            .build();
        assert_eq!(schema.extensions, vec!["uuid-ossp", "pgcrypto"]);
    }

    #[test]
    fn test_schema_builder_enum_type() {
        let schema = SchemaBuilder::new("public", SchemaDialect::Postgres)
            .enum_type_values("status", vec!["active", "inactive"])
            .build();

        assert!(schema.enums.contains_key("status"));
        assert_eq!(
            schema.enums.get("status").unwrap().values,
            vec!["active", "inactive"]
        );
    }

    #[test]
    fn test_schema_builder_add_enum() {
        let schema = SchemaBuilder::new("public", SchemaDialect::Postgres)
            .enum_type(EnumBuilder::new("role").value("admin").value("user"))
            .build();

        assert!(schema.enums.contains_key("role"));
    }

    #[test]
    fn test_schema_builder_table() {
        let schema = SchemaBuilder::new("public", SchemaDialect::Postgres)
            .table("users", |t| {
                t.column(ColumnBuilder::serial("id").primary_key())
            })
            .build();

        assert!(schema.tables.contains_key("users"));
        assert!(
            schema
                .tables
                .get("users")
                .unwrap()
                .columns
                .contains_key("id")
        );
    }

    // ==================== SchemaDialect Tests ====================

    #[test]
    fn test_schema_dialect_display() {
        assert_eq!(format!("{}", SchemaDialect::Postgres), "postgresql");
        assert_eq!(format!("{}", SchemaDialect::Mysql), "mysql");
        assert_eq!(format!("{}", SchemaDialect::Sqlite), "sqlite");
    }

    #[test]
    fn test_schema_dialect_default() {
        assert_eq!(SchemaDialect::default(), SchemaDialect::Postgres);
    }
}
