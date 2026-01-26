//! Table definitions

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::{Column, ColumnBuilder, Constraint, Index};

/// A database table definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Table name
    pub name: String,

    /// Schema name (e.g., "public" for PostgreSQL)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Columns in the table
    #[serde(default)]
    pub columns: IndexMap<String, Column>,

    /// Table constraints
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,

    /// Indexes on the table
    #[serde(default)]
    pub indexes: IndexMap<String, Index>,

    /// Table comment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Table options (e.g., storage parameters)
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub options: IndexMap<String, String>,

    /// Tablespace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tablespace: Option<String>,

    /// Partitioning specification (PostgreSQL)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<PartitionSpec>,
}

/// Table partitioning specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSpec {
    /// Partition method (RANGE, LIST, HASH)
    pub method: PartitionMethod,
    /// Partition columns or expressions
    pub columns: Vec<String>,
}

/// Partition method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PartitionMethod {
    Range,
    List,
    Hash,
}

impl Table {
    /// Create a new table
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            columns: IndexMap::new(),
            constraints: Vec::new(),
            indexes: IndexMap::new(),
            comment: None,
            options: IndexMap::new(),
            tablespace: None,
            partition: None,
        }
    }

    /// Create a new table in a schema
    pub fn in_schema(name: impl Into<String>, schema: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: Some(schema.into()),
            columns: IndexMap::new(),
            constraints: Vec::new(),
            indexes: IndexMap::new(),
            comment: None,
            options: IndexMap::new(),
            tablespace: None,
            partition: None,
        }
    }

    /// Get the fully qualified table name
    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(s) => format!("\"{}\".\"{}\"", s, self.name),
            None => format!("\"{}\"", self.name),
        }
    }

    /// Add a column to the table
    pub fn column(&mut self, column: Column) -> &mut Self {
        self.columns.insert(column.name.clone(), column);
        self
    }

    /// Add a constraint to the table
    pub fn constraint(&mut self, constraint: Constraint) -> &mut Self {
        self.constraints.push(constraint);
        self
    }

    /// Add an index to the table
    pub fn index(&mut self, index: Index) -> &mut Self {
        self.indexes.insert(index.name.clone(), index);
        self
    }

    /// Get a column by name
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.get(name)
    }

    /// Get a mutable column by name
    pub fn column_mut(&mut self, name: &str) -> Option<&mut Column> {
        self.columns.get_mut(name)
    }

    /// Get the primary key columns
    pub fn primary_key_columns(&self) -> Vec<&str> {
        // First check column-level primary keys
        let mut pk_cols: Vec<&str> = self
            .columns
            .values()
            .filter(|c| c.primary_key)
            .map(|c| c.name.as_str())
            .collect();

        // Then check table-level primary key constraint
        if pk_cols.is_empty() {
            for constraint in &self.constraints {
                if let Constraint::PrimaryKey(pk) = constraint {
                    pk_cols = pk.columns.iter().map(String::as_str).collect();
                    break;
                }
            }
        }

        pk_cols
    }

    /// Set the table comment
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Set an option
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.insert(key.into(), value.into());
        self
    }

    /// Set the tablespace
    pub fn tablespace(mut self, tablespace: impl Into<String>) -> Self {
        self.tablespace = Some(tablespace.into());
        self
    }

    /// Set partitioning
    pub fn partition_by(
        mut self,
        method: PartitionMethod,
        columns: Vec<impl Into<String>>,
    ) -> Self {
        self.partition = Some(PartitionSpec {
            method,
            columns: columns.into_iter().map(Into::into).collect(),
        });
        self
    }
}

// Allow ColumnBuilder to be converted to Column
impl From<ColumnBuilder> for Column {
    fn from(builder: ColumnBuilder) -> Self {
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TableBuilder;
    use crate::schema::constraint::PrimaryKeyConstraint;
    use crate::schema::index::IndexMethod;
    use crate::schema::types::DataType;
    use crate::schema::types::ReferenceAction;

    // ==================== Table Tests ====================

    #[test]
    fn test_table_new() {
        let table = Table::new("users");
        assert_eq!(table.name, "users");
        assert!(table.schema.is_none());
        assert!(table.columns.is_empty());
        assert!(table.constraints.is_empty());
        assert!(table.indexes.is_empty());
        assert!(table.comment.is_none());
    }

    #[test]
    fn test_table_in_schema() {
        let table = Table::in_schema("users", "custom");
        assert_eq!(table.name, "users");
        assert_eq!(table.schema, Some("custom".to_string()));
    }

    #[test]
    fn test_table_full_name_without_schema() {
        let table = Table::new("users");
        assert_eq!(table.full_name(), "\"users\"");
    }

    #[test]
    fn test_table_full_name_with_schema() {
        let table = Table::in_schema("users", "public");
        assert_eq!(table.full_name(), "\"public\".\"users\"");
    }

    #[test]
    fn test_table_add_column() {
        let mut table = Table::new("users");
        table.column(Column::new("id", DataType::Serial));
        table.column(Column::new("name", DataType::Text));

        assert_eq!(table.columns.len(), 2);
        assert!(table.columns.contains_key("id"));
        assert!(table.columns.contains_key("name"));
    }

    #[test]
    fn test_table_column_lookup() {
        let mut table = Table::new("users");
        table.column(Column::new("id", DataType::Serial));

        assert!(table.get_column("id").is_some());
        assert!(table.get_column("nonexistent").is_none());
    }

    #[test]
    fn test_table_column_mut() {
        let mut table = Table::new("users");
        table.column(Column::new("id", DataType::Serial));

        let col = table.column_mut("id").expect("column 'id' not found");
        col.nullable = false;

        assert!(
            !table
                .get_column("id")
                .expect("column 'id' not found")
                .nullable
        );
    }

    #[test]
    fn test_table_add_index() {
        let mut table = Table::new("users");
        table.index(Index::new("users_email_idx", vec!["email"]));

        assert_eq!(table.indexes.len(), 1);
        assert!(table.indexes.contains_key("users_email_idx"));
    }

    #[test]
    fn test_table_comment() {
        let table = Table::new("users").comment("User accounts");
        assert_eq!(table.comment, Some("User accounts".to_string()));
    }

    #[test]
    fn test_table_tablespace() {
        let table = Table::new("users").tablespace("fast_ssd");
        assert_eq!(table.tablespace, Some("fast_ssd".to_string()));
    }

    #[test]
    fn test_table_option() {
        let table = Table::new("users")
            .option("fillfactor", "90")
            .option("autovacuum_enabled", "false");

        assert_eq!(table.options.len(), 2);
        assert_eq!(table.options.get("fillfactor"), Some(&"90".to_string()));
    }

    #[test]
    fn test_table_partition_by() {
        let table = Table::new("events").partition_by(PartitionMethod::Range, vec!["created_at"]);

        assert!(table.partition.is_some());
        let partition = table.partition.expect("partition config not set");
        assert_eq!(partition.method, PartitionMethod::Range);
        assert_eq!(partition.columns, vec!["created_at"]);
    }

    #[test]
    fn test_table_primary_key_columns_from_column() {
        let mut table = Table::new("users");
        table.column(Column::new("id", DataType::Serial).primary_key());
        table.column(Column::new("name", DataType::Text));

        assert_eq!(table.primary_key_columns(), vec!["id"]);
    }

    #[test]
    fn test_table_primary_key_columns_from_constraint() {
        let mut table = Table::new("post_tags");
        table.column(Column::new("post_id", DataType::Integer));
        table.column(Column::new("tag_id", DataType::Integer));
        table.constraint(Constraint::PrimaryKey(PrimaryKeyConstraint::new(vec![
            "post_id", "tag_id",
        ])));

        let pk_cols = table.primary_key_columns();
        assert_eq!(pk_cols.len(), 2);
        assert!(pk_cols.contains(&"post_id"));
        assert!(pk_cols.contains(&"tag_id"));
    }

    // ==================== TableBuilder Tests ====================

    #[test]
    fn test_table_builder_new() {
        let table = TableBuilder::new("users").build();
        assert_eq!(table.name, "users");
        assert!(table.columns.is_empty());
    }

    #[test]
    fn test_table_builder_schema() {
        let table = TableBuilder::new("users").schema("custom").build();
        assert_eq!(table.schema, Some("custom".to_string()));
    }

    #[test]
    fn test_table_builder_add_column() {
        let table = TableBuilder::new("users")
            .column(ColumnBuilder::serial("id").primary_key())
            .column(ColumnBuilder::text("name").not_null())
            .build();

        assert_eq!(table.columns.len(), 2);
        assert!(
            table
                .columns
                .get("id")
                .expect("column 'id' not found")
                .primary_key
        );
        assert!(
            !table
                .columns
                .get("name")
                .expect("column 'name' not found")
                .nullable
        );
    }

    #[test]
    fn test_table_builder_column_with_closure() {
        let table = TableBuilder::new("users")
            .column_fn("email", DataType::Text, |c| c.not_null().unique())
            .build();

        let email_col = table
            .columns
            .get("email")
            .expect("column 'email' not found");
        assert!(!email_col.nullable);
        assert!(email_col.unique);
    }

    #[test]
    fn test_table_builder_comment() {
        let table = TableBuilder::new("users")
            .comment("User accounts table")
            .build();
        assert_eq!(table.comment, Some("User accounts table".to_string()));
    }

    #[test]
    fn test_table_builder_description() {
        let table = TableBuilder::new("users")
            .description("User accounts table")
            .build();
        assert_eq!(table.comment, Some("User accounts table".to_string()));
    }

    #[test]
    fn test_table_builder_description_multiline() {
        let table = TableBuilder::new("users")
            .description("User accounts.\n\nStores authentication info.")
            .build();
        assert_eq!(
            table.comment,
            Some("User accounts.\n\nStores authentication info.".to_string())
        );
    }

    #[test]
    fn test_table_builder_primary_key() {
        let table = TableBuilder::new("post_tags")
            .column(ColumnBuilder::integer("post_id").not_null())
            .column(ColumnBuilder::integer("tag_id").not_null())
            .primary_key(vec!["post_id", "tag_id"])
            .build();

        assert_eq!(table.constraints.len(), 1);
        if let Constraint::PrimaryKey(pk) = &table.constraints[0] {
            assert_eq!(pk.columns, vec!["post_id", "tag_id"]);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_table_builder_unique_constraint() {
        let table = TableBuilder::new("users")
            .column(ColumnBuilder::text("email"))
            .column(ColumnBuilder::text("username"))
            .unique_constraint(vec!["email", "username"])
            .build();

        assert!(
            table
                .constraints
                .iter()
                .any(|c| matches!(c, Constraint::Unique(_)))
        );
    }

    #[test]
    fn test_table_builder_unique_constraint_named() {
        let table = TableBuilder::new("users")
            .unique_constraint_named("uq_users_email", vec!["email"])
            .build();

        if let Constraint::Unique(u) = &table.constraints[0] {
            assert_eq!(u.name, Some("uq_users_email".to_string()));
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_table_builder_foreign_key() {
        let table = TableBuilder::new("posts")
            .column(ColumnBuilder::integer("author_id").not_null())
            .foreign_key(vec!["author_id"], "users", vec!["id"])
            .build();

        assert!(
            table
                .constraints
                .iter()
                .any(|c| matches!(c, Constraint::ForeignKey(_)))
        );
    }

    #[test]
    fn test_table_builder_foreign_key_with_actions() {
        let table = TableBuilder::new("posts")
            .column(ColumnBuilder::integer("author_id").not_null())
            .foreign_key_with_actions(
                vec!["author_id"],
                "users",
                vec!["id"],
                ReferenceAction::Cascade,
                ReferenceAction::NoAction,
            )
            .build();

        if let Constraint::ForeignKey(fk) = &table.constraints[0] {
            assert_eq!(fk.on_delete, ReferenceAction::Cascade);
            assert_eq!(fk.on_update, ReferenceAction::NoAction);
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_table_builder_check() {
        let table = TableBuilder::new("products")
            .column(ColumnBuilder::integer("price"))
            .check("price > 0")
            .build();

        if let Constraint::Check(c) = &table.constraints[0] {
            assert_eq!(c.expression, "price > 0");
            assert!(c.name.is_none());
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_table_builder_check_named() {
        let table = TableBuilder::new("products")
            .check_named("chk_positive_price", "price > 0")
            .build();

        if let Constraint::Check(c) = &table.constraints[0] {
            assert_eq!(c.name, Some("chk_positive_price".to_string()));
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_table_builder_index() {
        let table = TableBuilder::new("users")
            .column(ColumnBuilder::text("email"))
            .index("users_email_idx", vec!["email"])
            .build();

        assert!(table.indexes.contains_key("users_email_idx"));
        assert!(
            !table
                .indexes
                .get("users_email_idx")
                .expect("index 'users_email_idx' not found")
                .unique
        );
    }

    #[test]
    fn test_table_builder_unique_index() {
        let table = TableBuilder::new("users")
            .unique_index("users_email_unique", vec!["email"])
            .build();

        assert!(
            table
                .indexes
                .get("users_email_unique")
                .expect("index 'users_email_unique' not found")
                .unique
        );
    }

    #[test]
    fn test_table_builder_index_builder() {
        let table = TableBuilder::new("users")
            .index_builder("users_search_idx", |idx| {
                idx.column("name").column("email").using(IndexMethod::Gin)
            })
            .build();

        let index = table
            .indexes
            .get("users_search_idx")
            .expect("index 'users_search_idx' not found");
        assert_eq!(index.method, IndexMethod::Gin);
        assert_eq!(index.columns.len(), 2);
    }

    #[test]
    fn test_table_builder_full_example() {
        let table = TableBuilder::new("posts")
            .schema("blog")
            .description("Blog posts table")
            .column(ColumnBuilder::serial("id").primary_key())
            .column(
                ColumnBuilder::integer("author_id")
                    .not_null()
                    .description("FK to users table"),
            )
            .column(ColumnBuilder::text("title").not_null())
            .column(ColumnBuilder::text("content"))
            .column(ColumnBuilder::timestamptz("created_at").default_now())
            .foreign_key(vec!["author_id"], "users", vec!["id"])
            .index("posts_author_idx", vec!["author_id"])
            .check("length(title) > 0")
            .build();

        assert_eq!(table.name, "posts");
        assert_eq!(table.schema, Some("blog".to_string()));
        assert_eq!(table.comment, Some("Blog posts table".to_string()));
        assert_eq!(table.columns.len(), 5);
        assert_eq!(table.constraints.len(), 2); // FK + check
        assert_eq!(table.indexes.len(), 1);
    }

    // ==================== PartitionMethod Tests ====================

    #[test]
    fn test_partition_methods() {
        assert_eq!(PartitionMethod::Range, PartitionMethod::Range);
        assert_eq!(PartitionMethod::List, PartitionMethod::List);
        assert_eq!(PartitionMethod::Hash, PartitionMethod::Hash);
    }
}
