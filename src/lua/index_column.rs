//! Lua wrapper for IndexColumn

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};

use crate::schema::index::IndexColumn;

crate::lua_global_module! {
    metadata: INDEX_COLUMN_LUA_MODULE,
    register: register_index_column_module,
    name: "IndexColumn",
    doc: "Index column descriptor.",
    functions: [
        fn "column"(name: String => ("string", "string", "Column name")) -> "IndexColumn" => index_column_column;
        fn "expression"(expr: String => ("string", "string", "SQL expression")) -> "IndexColumn" => index_column_expression;
    ],
}

crate::lua_builder_def! {
    target: LuaIndexColumn,
    metadata: INDEX_COLUMN_LUA_TYPE,
    register: register_lua_index_column_methods,
    type_name: "IndexColumn",
    doc: "Index column descriptor.",
    fields: [],
    methods: [
        method "asc"() -> "IndexColumn" => |this| { let mut new = this.clone(); new.inner = new.inner.asc(); Ok(new) };
        method "desc"() -> "IndexColumn" => |this| { let mut new = this.clone(); new.inner = new.inner.desc(); Ok(new) };
        method "nulls_first"() -> "IndexColumn" => |this| { let mut new = this.clone(); new.inner = new.inner.nulls_first(); Ok(new) };
        method "nulls_last"() -> "IndexColumn" => |this| { let mut new = this.clone(); new.inner = new.inner.nulls_last(); Ok(new) };
        method "opclass"(opclass: String => ("string", "string", "Operator class name")) -> "IndexColumn" => |this, opclass| { let mut new = this.clone(); new.inner = new.inner.opclass(opclass); Ok(new) };
    ],
}

/// Lua wrapper for IndexColumn with ordering options
#[derive(Clone)]
pub struct LuaIndexColumn {
    inner: IndexColumn,
}

impl LuaIndexColumn {
    pub fn column(name: String) -> Self {
        Self {
            inner: IndexColumn::column(name),
        }
    }

    pub fn expression(expr: String) -> Self {
        Self {
            inner: IndexColumn::expression(expr),
        }
    }

    pub fn into_inner(self) -> IndexColumn {
        self.inner
    }
}

impl UserData for LuaIndexColumn {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_index_column_methods(methods);
    }
}

impl FromLua for LuaIndexColumn {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaIndexColumn".to_string(),
                message: Some("expected IndexColumn".to_string()),
            }),
        }
    }
}

/// IndexColumn.column(name) -> LuaIndexColumn
pub fn index_column_column(_lua: &Lua, name: String) -> LuaResult<LuaIndexColumn> {
    Ok(LuaIndexColumn::column(name))
}

/// IndexColumn.expression(expr) -> LuaIndexColumn
pub fn index_column_expression(_lua: &Lua, expr: String) -> LuaResult<LuaIndexColumn> {
    Ok(LuaIndexColumn::expression(expr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_index_column_metadata_matches_runtime_api() {
        assert_eq!(INDEX_COLUMN_LUA_MODULE.name, "IndexColumn");
        assert!(
            INDEX_COLUMN_LUA_MODULE
                .methods
                .iter()
                .any(|m| m.name == "column")
        );
        assert!(
            INDEX_COLUMN_LUA_TYPE
                .methods
                .iter()
                .any(|m| m.name == "opclass")
        );
        assert!(
            INDEX_COLUMN_LUA_TYPE
                .methods
                .iter()
                .any(|m| m.name == "nulls_last")
        );
    }
}
