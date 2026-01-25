use crate::schema::SchemaDialect;

/// Generate Lua schema template
pub fn lua_schema_template(dialect: SchemaDialect) -> String {
    let (dialect_mod, schema_arg) = match dialect {
        SchemaDialect::Postgres => ("pg", r#"("public")"#),
        SchemaDialect::Mysql => ("mysql", r#"("mydb")"#),
        SchemaDialect::Sqlite => ("sqlite", "()"),
    };

    format!(
        r#"--- Database Schema Definition
---
--- Define your database schema here using the shki Lua API.
--- Run `shki generate --schema schema/init.lua` to create migrations.
---
--- For IDE support (autocomplete, type checking, hover docs), install the
--- Lua Language Server extension. The .luarc.json is already configured.

local schema = {dialect_mod}.schema{schema_arg}

-- Example: Define an enum type (PostgreSQL only)
-- schema:enum_type(
--     EnumBuilder.new("status")
--         :description("Record status")
--         :value("active")
--         :value("inactive")
--         :value("archived")
-- )

-- Example: Define a users table
-- schema:table(
--     TableBuilder.new("users")
--         :description("User accounts")
--         :column(ColumnBuilder.serial("id"):primary_key())
--         :column(ColumnBuilder.text("email"):not_null():unique())
--         :column(ColumnBuilder.text("name"):not_null())
--         :column(ColumnBuilder.timestamptz("created_at"):default_now())
-- )

return schema
"#
    )
}
