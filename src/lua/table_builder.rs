//! Lua wrapper for TableBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{Table, TableBuilder};

use super::helpers::parse_referential_action;
use super::{LuaColumnBuilder, LuaIndexBuilder};

crate::lua_global_module! {
    metadata: TABLE_BUILDER_LUA_MODULE,
    register: register_table_builder_module,
    name: "TableBuilder",
    doc: "Builder for tables.",
    functions: [
        fn "new"(name: String => ("string", "string", "Name")) -> "TableBuilder" => table_builder_new;
    ],
}

crate::lua_builder_def! {
    target: LuaTableBuilder,
    metadata: TABLE_BUILDER_LUA_TYPE,
    register: register_lua_table_builder_methods,
    type_name: "TableBuilder",
    doc: "Builder for tables.",
    fields: [],
    methods: [
        method "schema"(schema: String => ("string", "string", "Schema name")) -> "TableBuilder" => |this, schema| { this.transform(|builder| builder.schema(schema)); Ok(this.clone()) };
        method "description"(desc: String => ("string", "string", "Description text")) -> "TableBuilder" => |this, desc| { this.transform(|builder| builder.description(desc)); Ok(this.clone()) };
        method "comment"(comment: String => ("string", "string", "Comment text")) -> "TableBuilder" => |this, comment| { this.transform(|builder| builder.comment(comment)); Ok(this.clone()) };
        method "column"(column: LuaColumnBuilder => ("ColumnBuilder", "any", "Column builder")) -> "TableBuilder" => |this, column| { let col = column.build(); this.transform(|builder| builder.column(col)); Ok(this.clone()) };
        method "primary_key"(columns: Vec<String> => ("string[]", "table", "Column names")) -> "TableBuilder" => |this, columns| { this.transform(|builder| builder.primary_key(columns)); Ok(this.clone()) };
        method "unique_constraint"(columns: Vec<String> => ("string[]", "table", "Column names")) -> "TableBuilder" => |this, columns| { this.transform(|builder| builder.unique_constraint(columns)); Ok(this.clone()) };
        method "foreign_key"(columns: Vec<String> => ("string[]", "table", "Local columns"), ref_table: String => ("string", "string", "Referenced table"), ref_columns: Vec<String> => ("string[]", "table", "Referenced columns")) -> "TableBuilder" => |this, columns, ref_table, ref_columns| { this.transform(|builder| builder.foreign_key(columns, ref_table, ref_columns)); Ok(this.clone()) };
        method "foreign_key_with_actions"(columns: Vec<String> => ("string[]", "table", "Local columns"), ref_table: String => ("string", "string", "Referenced table"), ref_columns: Vec<String> => ("string[]", "table", "Referenced columns"), on_delete: String => ("ReferenceAction|string", "string", "ON DELETE action"), on_update: String => ("ReferenceAction|string", "string", "ON UPDATE action")) -> "TableBuilder" => |this, columns, ref_table, ref_columns, on_delete, on_update| { let on_delete = parse_referential_action(&on_delete); let on_update = parse_referential_action(&on_update); this.transform(|builder| builder.foreign_key_with_actions(columns, ref_table, ref_columns, on_delete, on_update)); Ok(this.clone()) };
        method "check"(expression: String => ("string", "string", "SQL expression")) -> "TableBuilder" => |this, expression| { this.transform(|builder| builder.check(expression)); Ok(this.clone()) };
        method "index"(name: String => ("string", "string", "Index name"), columns: Vec<String> => ("string[]", "table", "Indexed columns")) -> "TableBuilder" => |this, name, columns| { this.transform(|builder| builder.index(name, columns)); Ok(this.clone()) };
        method "unique_index"(name: String => ("string", "string", "Index name"), columns: Vec<String> => ("string[]", "table", "Indexed columns")) -> "TableBuilder" => |this, name, columns| { this.transform(|builder| builder.unique_index(name, columns)); Ok(this.clone()) };
        method "index_with"(index: LuaIndexBuilder => ("IndexBuilder", "any", "Index builder")) -> "TableBuilder" => |this, index| { let idx = index.build(); this.transform(|builder| builder.index_with(idx)); Ok(this.clone()) };
    ],
}

/// Lua wrapper for TableBuilder
#[derive(Clone)]
pub struct LuaTableBuilder {
    inner: Rc<RefCell<TableBuilder>>,
}

impl LuaTableBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(TableBuilder::new(name))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(TableBuilder) -> TableBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> Table {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaTableBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_table_builder_methods(methods);
    }
}

impl FromLua for LuaTableBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaTableBuilder".to_string(),
                message: Some("expected TableBuilder".to_string()),
            }),
        }
    }
}

/// TableBuilder.new(name) -> LuaTableBuilder
pub fn table_builder_new(_: &Lua, name: String) -> LuaResult<LuaTableBuilder> {
    Ok(LuaTableBuilder::new(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_table_builder_metadata_matches_runtime_api() {
        assert_eq!(TABLE_BUILDER_LUA_MODULE.name, "TableBuilder");
        assert!(TABLE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "column"));
        assert!(TABLE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "foreign_key_with_actions"));
        assert!(TABLE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "index_with"));
    }
}
