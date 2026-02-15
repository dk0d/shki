//! Lua wrapper for IndexColumn

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};

use crate::schema::index::IndexColumn;

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
        // asc() -> self
        methods.add_method("asc", |_, this, ()| {
            let mut new = this.clone();
            new.inner = new.inner.asc();
            Ok(new)
        });

        // desc() -> self
        methods.add_method("desc", |_, this, ()| {
            let mut new = this.clone();
            new.inner = new.inner.desc();
            Ok(new)
        });

        // nulls_first() -> self
        methods.add_method("nulls_first", |_, this, ()| {
            let mut new = this.clone();
            new.inner = new.inner.nulls_first();
            Ok(new)
        });

        // nulls_last() -> self
        methods.add_method("nulls_last", |_, this, ()| {
            let mut new = this.clone();
            new.inner = new.inner.nulls_last();
            Ok(new)
        });

        // opclass(name) -> self
        methods.add_method("opclass", |_, this, opclass: String| {
            let mut new = this.clone();
            new.inner = new.inner.opclass(opclass);
            Ok(new)
        });
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
