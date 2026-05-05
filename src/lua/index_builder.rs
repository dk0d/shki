//! Lua wrapper for IndexBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::Index;
use crate::schema::IndexBuilder;

use super::helpers::parse_index_method;
use super::LuaIndexColumn;

crate::lua_global_module! {
    metadata: INDEX_BUILDER_LUA_MODULE,
    register: register_index_builder_module,
    name: "IndexBuilder",
    doc: "Builder for indexes.",
    functions: [
        fn "new"(name: String => ("string", "string", "Name")) -> "IndexBuilder" => index_builder_new;
    ],
}

crate::lua_builder_def! {
    target: LuaIndexBuilder,
    metadata: INDEX_BUILDER_LUA_TYPE,
    register: register_lua_index_builder_methods,
    type_name: "IndexBuilder",
    doc: "Builder for indexes.",
    fields: [],
    methods: [
        method "column"(name: String => ("string", "string", "Column name")) -> "IndexBuilder" => |this, name| { this.transform(|builder| builder.column(name)); Ok(this.clone()) };
        method "columns"(names: Vec<String> => ("string[]", "table", "Column names")) -> "IndexBuilder" => |this, names| { this.transform(|builder| builder.columns(names)); Ok(this.clone()) };
        method "expression"(expr: String => ("string", "string", "SQL expression")) -> "IndexBuilder" => |this, expr| { this.transform(|builder| builder.expression(expr)); Ok(this.clone()) };
        method "index_column"(col: LuaIndexColumn => ("IndexColumn", "any", "Index column descriptor")) -> "IndexBuilder" => |this, col| { this.transform(|builder| builder.index_column(col.into_inner())); Ok(this.clone()) };
        method "unique"() -> "IndexBuilder" => |this| { this.transform(|builder| builder.unique()); Ok(this.clone()) };
        method "using"(method: String => ("IndexMethod|string", "string", "Index method")) -> "IndexBuilder" => |this, method| { this.transform(|builder| builder.using(parse_index_method(&method))); Ok(this.clone()) };
        method "where_clause"(clause: String => ("string", "string", "WHERE clause without WHERE")) -> "IndexBuilder" => |this, clause| { this.transform(|builder| builder.where_clause(clause)); Ok(this.clone()) };
        method "include"(columns: Vec<String> => ("string[]", "table", "Column names")) -> "IndexBuilder" => |this, columns| { this.transform(|builder| builder.include(columns)); Ok(this.clone()) };
        method "concurrently"() -> "IndexBuilder" => |this| { this.transform(|builder| builder.concurrently()); Ok(this.clone()) };
        method "tablespace"(tablespace: String => ("string", "string", "Tablespace name")) -> "IndexBuilder" => |this, tablespace| { this.transform(|builder| builder.tablespace(tablespace)); Ok(this.clone()) };
        method "option"(key: String => ("string", "string", "Option key"), value: String => ("string", "string", "Option value")) -> "IndexBuilder" => |this, key, value| { this.transform(|builder| builder.option(key, value)); Ok(this.clone()) };
    ],
}

/// Lua wrapper for IndexBuilder
#[derive(Clone)]
pub struct LuaIndexBuilder {
    inner: Rc<RefCell<IndexBuilder>>,
}

impl LuaIndexBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(IndexBuilder::new(name))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(IndexBuilder) -> IndexBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> Index {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaIndexBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_index_builder_methods(methods);
    }
}

impl FromLua for LuaIndexBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaIndexBuilder".to_string(),
                message: Some("expected IndexBuilder".to_string()),
            }),
        }
    }
}

/// IndexBuilder.new(name) -> LuaIndexBuilder
pub fn index_builder_new(_: &Lua, name: String) -> LuaResult<LuaIndexBuilder> {
    Ok(LuaIndexBuilder::new(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_index_builder_metadata_matches_runtime_api() {
        assert_eq!(INDEX_BUILDER_LUA_MODULE.name, "IndexBuilder");
        assert!(INDEX_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "index_column"));
        assert!(INDEX_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "using"));
        assert!(INDEX_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "concurrently"));
    }
}
