//! Lua schema definition support
//!
//! This module allows defining database schemas using Lua scripts that mirror
//! the Rust builder patterns.
//!
//! # Example
//!
//! ```lua
//! -- schema.lua
//!
//! -- Define a PostgreSQL schema
//! local schema = pg.schema("public")
//! local post_status = require("post_status")
//!
//! -- Define an enum
//! schema:enum(post_status)
//!
//! -- Define a table
//! schema:table(
//!     TableBuilder.new("posts")
//!         :description("Blog posts")
//!         :column(ColumnBuilder.serial("id"):primary_key())
//!         :column(ColumnBuilder.text("title"):not_null())
//!         :column(ColumnBuilder.enum("status", post_status):not_null())
//! )
//!
//! return schema
//! ```
//!
//! ```lua
//! -- post_status.lua
//! return EnumBuilder.new("post_status")
//!     :description("Status of a blog post")
//!     :value("draft")
//!     :value("published")
//!     :value("archived")
//! ```

mod column_builder;
mod enum_builder;
mod helpers;
mod index_builder;
mod index_column;
mod schema;
mod sequence_builder;
mod table_builder;
mod view_builder;

// Re-export all types and functions from individual modules
pub use column_builder::*;
pub use enum_builder::*;
pub use helpers::*;
pub use index_builder::*;
pub use index_column::*;
pub use schema::*;
pub use sequence_builder::*;
pub use table_builder::*;
pub use view_builder::*;

use crate::schema::Schema;
use crate::{Result, ShkiError};
use mlua::{Lua, Result as LuaResult};
use std::path::Path;

/// Load a schema from a Lua script file
pub fn load_schema_from_file(path: &Path) -> Result<Schema> {
    let script = std::fs::read_to_string(path)?;

    // Get the directory containing the schema file to set up package.path
    let schema_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    load_schema_from_str_with_path(&script, path.to_string_lossy().as_ref(), &schema_dir)
}

/// Load a schema from a Lua script string
pub fn load_schema_from_str(script: &str, name: &str) -> Result<Schema> {
    load_schema_from_str_with_path(script, name, ".")
}

/// Load a schema from a Lua script string with a custom package path
fn load_schema_from_str_with_path(script: &str, name: &str, search_path: &str) -> Result<Schema> {
    let lua = create_lua_runtime()?;

    // Set up package.path to include the schema directory
    // This allows require() to find modules in the same directory as the schema file
    let setup_path = format!(
        r#"package.path = "{}/lua/?.lua;{}/?.lua;{}/?.init.lua;./lua/?.lua;" .. package.path"#,
        search_path.replace('\\', "/"),
        search_path.replace('\\', "/"),
        search_path.replace('\\', "/"),
    );

    lua.load(&setup_path)
        .exec()
        .map_err(|e| ShkiError::lua(format!("Failed to set package.path: {}", e)))?;

    let result: LuaResult<LuaSchema> = lua.scope(|_scope| lua.load(script).set_name(name).eval());

    match result {
        Ok(lua_schema) => Ok(lua_schema.into_schema()),
        Err(e) => Err(ShkiError::lua(format!("Lua error in {}: {}", name, e))),
    }
}

/// Create a new Lua runtime with all schema bindings registered
pub fn create_lua_runtime() -> Result<Lua> {
    let lua = Lua::new();
    register_schema_bindings(&lua)?;
    Ok(lua)
}

/// Register all schema-related bindings in the Lua runtime
fn register_schema_bindings(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // Register pg module
    let pg = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    pg.set(
        "schema",
        lua.create_function(pg_schema)
            .map_err(|e| ShkiError::lua(e.to_string()))?,
    )
    .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set("pg", pg)
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register mysql module
    let mysql = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    mysql
        .set(
            "schema",
            lua.create_function(mysql_schema)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set("mysql", mysql)
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register sqlite module
    let sqlite = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    sqlite
        .set(
            "schema",
            lua.create_function(sqlite_schema)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set("sqlite", sqlite)
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register EnumBuilder
    globals
        .set(
            "EnumBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let enum_builder = globals
        .get::<mlua::Table>("EnumBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    enum_builder
        .set(
            "new",
            lua.create_function(enum_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register TableBuilder
    globals
        .set(
            "TableBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let table_builder = globals
        .get::<mlua::Table>("TableBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    table_builder
        .set(
            "new",
            lua.create_function(table_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register ColumnBuilder
    globals
        .set(
            "ColumnBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let column_builder = globals
        .get::<mlua::Table>("ColumnBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Type constructors
    column_builder
        .set(
            "new",
            lua.create_function(column_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "serial",
            lua.create_function(column_builder_serial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bigserial",
            lua.create_function(column_builder_bigserial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "smallserial",
            lua.create_function(column_builder_smallserial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "integer",
            lua.create_function(column_builder_integer)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bigint",
            lua.create_function(column_builder_bigint)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "smallint",
            lua.create_function(column_builder_smallint)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "text",
            lua.create_function(column_builder_text)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "varchar",
            lua.create_function(column_builder_varchar)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "char",
            lua.create_function(column_builder_char)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "boolean",
            lua.create_function(column_builder_boolean)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "timestamp",
            lua.create_function(column_builder_timestamp)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "timestamptz",
            lua.create_function(column_builder_timestamptz)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "date",
            lua.create_function(column_builder_date)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "time",
            lua.create_function(column_builder_time)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "uuid",
            lua.create_function(column_builder_uuid)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "json",
            lua.create_function(column_builder_json)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "jsonb",
            lua.create_function(column_builder_jsonb)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "numeric",
            lua.create_function(column_builder_numeric)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "real",
            lua.create_function(column_builder_real)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "double_precision",
            lua.create_function(column_builder_double_precision)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bytea",
            lua.create_function(column_builder_bytea)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "inet",
            lua.create_function(column_builder_inet)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "cidr",
            lua.create_function(column_builder_cidr)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "enum",
            lua.create_function(column_builder_enum_type)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "array",
            lua.create_function(column_builder_array)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register IndexBuilder
    globals
        .set(
            "IndexBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let index_builder = globals
        .get::<mlua::Table>("IndexBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    index_builder
        .set(
            "new",
            lua.create_function(index_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register SequenceBuilder
    globals
        .set(
            "SequenceBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let sequence_builder = globals
        .get::<mlua::Table>("SequenceBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    sequence_builder
        .set(
            "new",
            lua.create_function(sequence_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register ViewBuilder
    globals
        .set(
            "ViewBuilder",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let view_builder = globals
        .get::<mlua::Table>("ViewBuilder")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    view_builder
        .set(
            "new",
            lua.create_function(view_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register IndexColumn
    globals
        .set(
            "IndexColumn",
            lua.create_table()
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    let index_column = globals
        .get::<mlua::Table>("IndexColumn")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    index_column
        .set(
            "column",
            lua.create_function(index_column_column)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    index_column
        .set(
            "expression",
            lua.create_function(index_column_expression)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register ReferenceAction enum
    let ref_action = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    ref_action
        .set("NoAction", "no_action")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    ref_action
        .set("Restrict", "restrict")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    ref_action
        .set("Cascade", "cascade")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    ref_action
        .set("SetNull", "set_null")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    ref_action
        .set("SetDefault", "set_default")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set("ReferenceAction", ref_action)
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    // Register IndexMethod enum
    let idx_method = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("BTree", "btree")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("Hash", "hash")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("Gist", "gist")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("SpGist", "spgist")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("Gin", "gin")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    idx_method
        .set("Brin", "brin")
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set("IndexMethod", idx_method)
        .map_err(|e| ShkiError::lua(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::types::DataType;
    use std::fs;

    #[test]
    fn test_lua_create_runtime() {
        let lua = create_lua_runtime().unwrap();

        // Verify globals are registered
        let globals = lua.globals();
        assert!(globals.get::<mlua::Table>("pg").is_ok());
        assert!(globals.get::<mlua::Table>("mysql").is_ok());
        assert!(globals.get::<mlua::Table>("sqlite").is_ok());
        assert!(globals.get::<mlua::Table>("EnumBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("TableBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("ColumnBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("IndexBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("SequenceBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("ViewBuilder").is_ok());
        assert!(globals.get::<mlua::Table>("IndexColumn").is_ok());
    }

    #[test]
    fn test_lua_simple_schema() {
        let script = r#"
            local schema = pg.schema("public")
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        assert_eq!(schema.name, "public");
    }

    #[test]
    fn test_lua_simple_schema_error() {
        let test_cases = [
            // Missing return statement
            r#"
            local schema = pg.schema("public")
            -- Fails to return schema
        "#,
            // Invalid column type
            r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("users")
                    :column(ColumnBuilder.unknown_type("id"))  -- Invalid column type
            )
            -- Fails to return schema
            return schema
        "#,
        ];

        for script in test_cases {
            let schema = load_schema_from_str(script, "test");
            assert!(schema.is_err());
        }
    }

    #[test]
    fn test_lua_schema_with_enum() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:enum(
                EnumBuilder.new("status")
                    :description("Item status")
                    :value("active")
                    :value("inactive")
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        assert!(schema.enums.contains_key("status"));
        let status_enum = schema.enums.get("status").unwrap();
        assert_eq!(status_enum.values, vec!["active", "inactive"]);
        assert_eq!(status_enum.description, Some("Item status".to_string()));
    }

    #[test]
    fn test_lua_schema_with_table() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("users")
                    :description("User accounts")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.text("name"):not_null())
                    :column(ColumnBuilder.text("email"):not_null():unique())
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        assert!(schema.tables.contains_key("users"));

        let users = schema.tables.get("users").unwrap();
        assert_eq!(users.comment, Some("User accounts".to_string()));
        assert!(users.columns.contains_key("id"));
        assert!(users.columns.contains_key("name"));
        assert!(users.columns.contains_key("email"));

        let id_col = users.columns.get("id").unwrap();
        assert!(id_col.primary_key);

        let email_col = users.columns.get("email").unwrap();
        assert!(email_col.unique);
        assert!(!email_col.nullable);
    }

    #[test]
    fn test_lua_full_schema() {
        let script = r#"
            local schema = pg.schema("public")
            local post_status = EnumBuilder.new("post_status")
                :value("draft")
                :value("published")
                :value("archived")
            
            -- Add enum
            schema:enum(post_status)
            
            -- Users table
            schema:table(
                TableBuilder.new("users")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.text("username"):not_null():unique())
                    :column(ColumnBuilder.text("email"):not_null():unique())
                    :column(ColumnBuilder.timestamptz("created_at"):default_now())
                    :index("users_email_idx", {"email"})
            )
            
            -- Posts table
            schema:table(
                TableBuilder.new("posts")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.integer("author_id"):not_null():references("users", "id"))
                    :column(ColumnBuilder.text("title"):not_null())
                    :column(ColumnBuilder.enum("status", post_status):not_null())
                    :column(ColumnBuilder.timestamptz("created_at"):default_now())
                    :index("posts_author_idx", {"author_id"})
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();

        assert_eq!(schema.enums.len(), 1);
        assert_eq!(schema.tables.len(), 2);

        let posts = schema.tables.get("posts").unwrap();
        assert!(posts.indexes.contains_key("posts_author_idx"));
    }

    #[test]
    fn test_lua_column_enum_type_accepts_enum_builder() {
        let script = r#"
            local schema = pg.schema("public")
            local post_status = EnumBuilder.new("post_status")
                :value("draft")
                :value("published")
                :value("archived")

            schema:enum(post_status)

            schema:table(
                TableBuilder.new("posts")
                    :column(ColumnBuilder.enum("status", post_status):not_null())
            )

            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let posts = schema.tables.get("posts").unwrap();
        let status = posts.columns.get("status").unwrap();
        let post_status = schema.enums.get("post_status").unwrap();

        assert!(matches!(
            &status.data_type,
            DataType::Enum { name, schema } if name == "post_status" && schema.is_none()
        ));
        assert_eq!(post_status.values, vec!["draft", "published", "archived"]);
    }

    #[test]
    fn test_lua_column_enum_type_accepts_required_enum_builder() {
        let dir = tempfile::tempdir().unwrap();
        let enum_path = dir.path().join("post_status.lua");
        let schema_path = dir.path().join("schema.lua");

        fs::write(
            &enum_path,
            r#"
                return EnumBuilder.new("post_status")
                    :value("draft")
                    :value("published")
                    :value("archived")
            "#,
        )
        .unwrap();

        fs::write(
            &schema_path,
            r#"
                local schema = pg.schema("public")
                local post_status = require("post_status")

                schema:enum(post_status)

                schema:table(
                    TableBuilder.new("posts")
                        :column(ColumnBuilder.enum("status", post_status):not_null())
                )

                return schema
            "#,
        )
        .unwrap();

        let schema = load_schema_from_file(&schema_path).unwrap();
        let posts = schema.tables.get("posts").unwrap();
        let status = posts.columns.get("status").unwrap();
        let post_status = schema.enums.get("post_status").unwrap();

        assert!(matches!(
            &status.data_type,
            DataType::Enum { name, schema } if name == "post_status" && schema.is_none()
        ));
        assert_eq!(post_status.values, vec!["draft", "published", "archived"]);
    }

    #[test]
    fn test_lua_column_enum_type_still_accepts_string_name() {
        let script = r#"
            local schema = pg.schema("public")

            schema:enum(
                EnumBuilder.new("post_status")
                    :value("draft")
                    :value("published")
            )

            schema:table(
                TableBuilder.new("posts")
                    :column(ColumnBuilder.enum("status", "post_status"):not_null())
            )

            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let posts = schema.tables.get("posts").unwrap();
        let status = posts.columns.get("status").unwrap();

        assert!(matches!(
            &status.data_type,
            DataType::Enum { name, schema } if name == "post_status" && schema.is_none()
        ));
    }

    #[test]
    fn test_lua_column_default_values() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("test_defaults")
                    :column(ColumnBuilder.text("col_literal"):default_value("'hello'"))
                    :column(ColumnBuilder.timestamptz("col_now"):default_now())
                    :column(ColumnBuilder.timestamptz("col_timestamp"):default_current_timestamp())
                    :column(ColumnBuilder.text("col_expr"):default_sql("upper('test')"))
                    :column(ColumnBuilder.text("col_null"):default_null())
                    :column(ColumnBuilder.uuid("col_uuid_v4"):default_uuid_generate_v4())
                    :column(ColumnBuilder.uuid("col_random_uuid"):default_gen_random_uuid())
                    :column(ColumnBuilder.uuid("col_uuidv7"):default_uuidv7())
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").expect("failed to load schema");
        let table = schema
            .tables
            .get("test_defaults")
            .expect("test_defaults table not found");

        // Check col_literal has literal default
        let col_literal = table
            .columns
            .get("col_literal")
            .expect("col_literal not found");
        assert_eq!(
            col_literal.default,
            Some(crate::schema::types::DefaultValue::Literal(
                "'hello'".to_string()
            ))
        );

        // Check col_now has now() expression
        let col_now = table.columns.get("col_now").expect("col_now not found");
        assert_eq!(
            col_now.default,
            Some(crate::schema::types::DefaultValue::Sql("now()".to_string()))
        );

        // Check col_timestamp has CURRENT_TIMESTAMP expression
        let col_timestamp = table
            .columns
            .get("col_timestamp")
            .expect("col_timestamp not found");
        assert_eq!(
            col_timestamp.default,
            Some(crate::schema::types::DefaultValue::Sql(
                "CURRENT_TIMESTAMP".to_string()
            ))
        );

        // Check col_expr has custom expression
        let col_expr = table.columns.get("col_expr").expect("col_expr not found");
        assert_eq!(
            col_expr.default,
            Some(crate::schema::types::DefaultValue::Sql(
                "upper('test')".to_string()
            ))
        );

        // Check col_null has NULL literal
        let col_null = table.columns.get("col_null").expect("col_null not found");
        assert_eq!(
            col_null.default,
            Some(crate::schema::types::DefaultValue::Literal(
                "NULL".to_string()
            ))
        );

        // Check col_uuid_v4 has uuid_generate_v4() expression
        let col_uuid_v4 = table
            .columns
            .get("col_uuid_v4")
            .expect("col_uuid_v4 not found");
        assert_eq!(
            col_uuid_v4.default,
            Some(crate::schema::types::DefaultValue::Sql(
                "uuid_generate_v4()".to_string()
            ))
        );

        // Check col_random_uuid has gen_random_uuid() expression
        let col_random_uuid = table
            .columns
            .get("col_random_uuid")
            .expect("col_random_uuid not found");
        assert_eq!(
            col_random_uuid.default,
            Some(crate::schema::types::DefaultValue::Sql(
                "gen_random_uuid()".to_string()
            ))
        );

        // Check col_uuidv7 has uuidv7() expression
        let col_uuidv7 = table
            .columns
            .get("col_uuidv7")
            .expect("col_uuidv7 not found");
        assert_eq!(
            col_uuidv7.default,
            Some(crate::schema::types::DefaultValue::Sql(
                "uuidv7()".to_string()
            ))
        );
    }

    #[test]
    fn test_lua_schema_with_sequence() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:sequence(
                SequenceBuilder.new("order_seq")
                    :start(1000)
                    :increment(1)
                    :cache(10)
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        assert!(schema.sequences.contains_key("order_seq"));

        let seq = schema.sequences.get("order_seq").unwrap();
        assert_eq!(seq.name, "order_seq");
        assert_eq!(seq.start, 1000);
        assert_eq!(seq.increment, 1);
        assert_eq!(seq.cache, 10);
    }

    #[test]
    fn test_lua_sequence_all_options() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:sequence(
                SequenceBuilder.new("custom_seq")
                    :schema("myschema")
                    :start(100)
                    :increment(5)
                    :min_value(1)
                    :max_value(10000)
                    :cache(20)
                    :cycle()
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let seq = schema.sequences.get("custom_seq").unwrap();

        assert_eq!(seq.schema, Some("myschema".to_string()));
        assert_eq!(seq.start, 100);
        assert_eq!(seq.increment, 5);
        assert_eq!(seq.min_value, 1);
        assert_eq!(seq.max_value, Some(10000));
        assert_eq!(seq.cache, 20);
        assert!(seq.cycle);
    }

    #[test]
    fn test_lua_sequence_no_cycle() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:sequence(
                SequenceBuilder.new("no_cycle_seq")
                    :cycle()
                    :no_cycle()
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let seq = schema.sequences.get("no_cycle_seq").unwrap();
        assert!(!seq.cycle);
    }

    #[test]
    fn test_lua_schema_with_view() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:view(
                ViewBuilder.new("active_users", "SELECT * FROM users WHERE is_active = true")
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        assert!(schema.views.contains_key("active_users"));

        let view = schema.views.get("active_users").unwrap();
        assert_eq!(view.name, "active_users");
        assert_eq!(
            view.definition,
            "SELECT * FROM users WHERE is_active = true"
        );
        assert!(!view.materialized);
    }

    #[test]
    fn test_lua_view_materialized() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:view(
                ViewBuilder.new("user_stats", "SELECT user_id, COUNT(*) FROM orders GROUP BY user_id")
                    :materialized()
                    :schema("analytics")
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let view = schema.views.get("user_stats").unwrap();

        assert!(view.materialized);
        assert_eq!(view.schema, Some("analytics".to_string()));
    }

    #[test]
    fn test_lua_index_with_tablespace_and_options() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("users")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.text("email"):not_null())
                    :index_with(
                        IndexBuilder.new("users_email_idx")
                            :column("email")
                            :tablespace("fast_ssd")
                            :option("fillfactor", "90")
                    )
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let users = schema.tables.get("users").unwrap();
        let idx = users.indexes.get("users_email_idx").unwrap();

        assert_eq!(idx.tablespace, Some("fast_ssd".to_string()));
        assert_eq!(
            idx.options,
            vec![("fillfactor".to_string(), "90".to_string())]
        );
    }

    #[test]
    fn test_lua_index_column_ordering() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("events")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.timestamptz("created_at"):not_null())
                    :column(ColumnBuilder.text("name"))
                    :index_with(
                        IndexBuilder.new("events_created_idx")
                            :index_column(IndexColumn.column("created_at"):desc():nulls_last())
                            :index_column(IndexColumn.column("name"):asc():nulls_first())
                    )
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let events = schema.tables.get("events").unwrap();
        let idx = events.indexes.get("events_created_idx").unwrap();

        assert_eq!(idx.columns.len(), 2);

        // Check first column (created_at DESC NULLS LAST)
        if let crate::schema::index::IndexColumn::Column {
            name, order, nulls, ..
        } = &idx.columns[0]
        {
            assert_eq!(name, "created_at");
            assert_eq!(*order, Some(crate::schema::index::SortOrder::Desc));
            assert_eq!(*nulls, Some(crate::schema::index::NullsOrder::Last));
        } else {
            panic!("Expected Column variant");
        }

        // Check second column (name ASC NULLS FIRST)
        if let crate::schema::index::IndexColumn::Column {
            name, order, nulls, ..
        } = &idx.columns[1]
        {
            assert_eq!(name, "name");
            assert_eq!(*order, Some(crate::schema::index::SortOrder::Asc));
            assert_eq!(*nulls, Some(crate::schema::index::NullsOrder::First));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_lua_index_column_expression() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("users")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.text("email"):not_null())
                    :index_with(
                        IndexBuilder.new("users_lower_email_idx")
                            :index_column(IndexColumn.expression("lower(email)"))
                    )
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let users = schema.tables.get("users").unwrap();
        let idx = users.indexes.get("users_lower_email_idx").unwrap();

        if let crate::schema::index::IndexColumn::Expression { expression, .. } = &idx.columns[0] {
            assert_eq!(expression, "lower(email)");
        } else {
            panic!("Expected Expression variant");
        }
    }

    #[test]
    fn test_lua_index_column_opclass() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("documents")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.text("content"):not_null())
                    :index_with(
                        IndexBuilder.new("documents_search_idx")
                            :index_column(IndexColumn.column("content"):opclass("gin_trgm_ops"))
                            :using("gin")
                    )
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let docs = schema.tables.get("documents").unwrap();
        let idx = docs.indexes.get("documents_search_idx").unwrap();

        assert_eq!(idx.method, crate::schema::index::IndexMethod::Gin);

        if let crate::schema::index::IndexColumn::Column { opclass, .. } = &idx.columns[0] {
            assert_eq!(*opclass, Some("gin_trgm_ops".to_string()));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_lua_parse_new_data_types() {
        let script = r#"
            local schema = pg.schema("public")
            
            schema:table(
                TableBuilder.new("test_types")
                    :column(ColumnBuilder.new("col_citext", "citext"))
                    :column(ColumnBuilder.new("col_money", "money"))
                    :column(ColumnBuilder.new("col_decimal", "decimal"))
                    :column(ColumnBuilder.new("col_interval", "interval"))
                    :column(ColumnBuilder.new("col_macaddr", "macaddr"))
                    :column(ColumnBuilder.new("col_point", "point"))
                    :column(ColumnBuilder.new("col_int4range", "int4range"))
                    :column(ColumnBuilder.new("col_daterange", "daterange"))
                    :column(ColumnBuilder.new("col_timetz", "timetz"))
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();
        let table = schema.tables.get("test_types").unwrap();

        // Verify each column has the correct type
        assert!(matches!(
            table.columns.get("col_citext").unwrap().data_type,
            crate::schema::types::DataType::Citext
        ));
        assert!(matches!(
            table.columns.get("col_money").unwrap().data_type,
            crate::schema::types::DataType::Money
        ));
        assert!(matches!(
            table.columns.get("col_decimal").unwrap().data_type,
            crate::schema::types::DataType::Decimal { .. }
        ));
        assert!(matches!(
            table.columns.get("col_interval").unwrap().data_type,
            crate::schema::types::DataType::Interval
        ));
        assert!(matches!(
            table.columns.get("col_macaddr").unwrap().data_type,
            crate::schema::types::DataType::MacAddr
        ));
        assert!(matches!(
            table.columns.get("col_point").unwrap().data_type,
            crate::schema::types::DataType::Point
        ));
        assert!(matches!(
            table.columns.get("col_int4range").unwrap().data_type,
            crate::schema::types::DataType::Int4Range
        ));
        assert!(matches!(
            table.columns.get("col_daterange").unwrap().data_type,
            crate::schema::types::DataType::DateRange
        ));
        assert!(matches!(
            table.columns.get("col_timetz").unwrap().data_type,
            crate::schema::types::DataType::Time {
                with_timezone: true,
                ..
            }
        ));
    }

    #[test]
    fn test_lua_full_schema_with_all_features() {
        let script = r#"
            local schema = pg.schema("public")
            local order_status = EnumBuilder.new("order_status")
                :description("Status of an order")
                :values({"pending", "processing", "shipped", "delivered"})
            
            -- Add extension
            schema:extension("uuid-ossp")
            
            -- Add enum
            schema:enum(order_status)
            
            -- Add sequence
            schema:sequence(
                SequenceBuilder.new("order_number_seq")
                    :start(1000)
                    :increment(1)
            )
            
            -- Add table with various features
            schema:table(
                TableBuilder.new("orders")
                    :description("Customer orders")
                    :column(ColumnBuilder.serial("id"):primary_key())
                    :column(ColumnBuilder.integer("order_number"):not_null():unique())
                    :column(ColumnBuilder.enum("status", order_status):not_null())
                    :column(ColumnBuilder.numeric("total", 10, 2):not_null())
                    :column(ColumnBuilder.timestamptz("created_at"):default_now())
                    :index_with(
                        IndexBuilder.new("orders_status_created_idx")
                            :index_column(IndexColumn.column("status"))
                            :index_column(IndexColumn.column("created_at"):desc())
                    )
            )
            
            -- Add view
            schema:view(
                ViewBuilder.new("pending_orders", "SELECT * FROM orders WHERE status = 'pending'")
            )
            
            return schema
        "#;

        let schema = load_schema_from_str(script, "test").unwrap();

        // Verify all components are present
        assert_eq!(schema.extensions, vec!["uuid-ossp"]);
        assert!(schema.enums.contains_key("order_status"));
        assert!(schema.sequences.contains_key("order_number_seq"));
        assert!(schema.tables.contains_key("orders"));
        assert!(schema.views.contains_key("pending_orders"));

        // Verify table details
        let orders = schema.tables.get("orders").unwrap();
        assert_eq!(orders.comment, Some("Customer orders".to_string()));
        assert!(orders.indexes.contains_key("orders_status_created_idx"));
    }
}
