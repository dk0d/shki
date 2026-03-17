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
--- Run `shki generate` in the project directory to create migrations.
---
--- For IDE support (autocomplete, type checking, hover docs), install the
--- Lua Language Server extension. The .luarc.json is already configured.

local schema = {dialect_mod}.schema{schema_arg}
local Table = TableBuilder
local Col = ColumnBuilder


-- Example: Define a users table
-- schema:table(
--     Table.new("users")
--         :description("User accounts")
--         :column(ColumnBuilder.serial("id"):primary_key())
--         :column(ColumnBuilder.text("email"):not_null():unique())
--         :column(ColumnBuilder.text("name"):not_null())
--         :column(ColumnBuilder.timestamptz("created_at"):default_now())
-- )

-- Example: Define an enum type (PostgreSQL only)
-- local status = require("status")
-- schema:enum(status)
-- schema:table(
--     Table.new("orders")
--         :column(Col.enum("status", status):not_null())
-- )
--
-- In `lua/status.lua`:
-- return EnumBuilder.new("status")
--     :description("Record status")
--     :value("active")
--     :value("inactive")
--     :value("archived")

return schema
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_schema_template_includes_reusable_enum_example() {
        let template = lua_schema_template(SchemaDialect::Postgres);

        assert!(template.contains("local status = require(\"status\")"));
        assert!(template.contains("schema:enum(status)"));
        assert!(template.contains(":column(Col.enum(\"status\", status):not_null())"));
        assert!(template.contains("return EnumBuilder.new(\"status\")"));
    }
}
