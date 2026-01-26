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
//!
//! -- Define an enum
//! schema:enum_type(
//!     EnumBuilder.new("post_status")
//!         :description("Status of a blog post")
//!         :value("draft")
//!         :value("published")
//!         :value("archived")
//! )
//!
//! -- Define a table
//! schema:table(
//!     TableBuilder.new("users")
//!         :description("User accounts")
//!         :column(ColumnBuilder.serial("id"):primary_key())
//!         :column(ColumnBuilder.text("email"):not_null():unique())
//!         :column(ColumnBuilder.timestamptz("created_at"):default_now())
//! )
//!
//! return schema
//! ```

mod bindings;

pub use bindings::*;

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
        r#"package.path = "{}/?.lua;{}/?.init.lua;" .. package.path"#,
        search_path.replace('\\', "/"),
        search_path.replace('\\', "/")
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
        lua.create_function(bindings::pg_schema)
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
            lua.create_function(bindings::mysql_schema)
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
            lua.create_function(bindings::sqlite_schema)
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
            lua.create_function(bindings::enum_builder_new)
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
            lua.create_function(bindings::table_builder_new)
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
            lua.create_function(bindings::column_builder_new)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "serial",
            lua.create_function(bindings::column_builder_serial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bigserial",
            lua.create_function(bindings::column_builder_bigserial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "smallserial",
            lua.create_function(bindings::column_builder_smallserial)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "integer",
            lua.create_function(bindings::column_builder_integer)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bigint",
            lua.create_function(bindings::column_builder_bigint)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "smallint",
            lua.create_function(bindings::column_builder_smallint)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "text",
            lua.create_function(bindings::column_builder_text)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "varchar",
            lua.create_function(bindings::column_builder_varchar)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "char",
            lua.create_function(bindings::column_builder_char)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "boolean",
            lua.create_function(bindings::column_builder_boolean)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "timestamp",
            lua.create_function(bindings::column_builder_timestamp)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "timestamptz",
            lua.create_function(bindings::column_builder_timestamptz)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "date",
            lua.create_function(bindings::column_builder_date)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "time",
            lua.create_function(bindings::column_builder_time)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "uuid",
            lua.create_function(bindings::column_builder_uuid)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "json",
            lua.create_function(bindings::column_builder_json)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "jsonb",
            lua.create_function(bindings::column_builder_jsonb)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "numeric",
            lua.create_function(bindings::column_builder_numeric)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "real",
            lua.create_function(bindings::column_builder_real)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "double_precision",
            lua.create_function(bindings::column_builder_double_precision)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "bytea",
            lua.create_function(bindings::column_builder_bytea)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "inet",
            lua.create_function(bindings::column_builder_inet)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "cidr",
            lua.create_function(bindings::column_builder_cidr)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "enum_type",
            lua.create_function(bindings::column_builder_enum_type)
                .map_err(|e| ShkiError::lua(e.to_string()))?,
        )
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    column_builder
        .set(
            "array",
            lua.create_function(bindings::column_builder_array)
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
            lua.create_function(bindings::index_builder_new)
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
            
            schema:enum_type(
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
            
            -- Add enum
            schema:enum_type(
                EnumBuilder.new("post_status")
                    :value("draft")
                    :value("published")
                    :value("archived")
            )
            
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
                    :column(ColumnBuilder.enum_type("status", "post_status"):not_null())
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
}
