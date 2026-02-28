//! Lua wrapper for IndexBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::Index;
use crate::schema::IndexBuilder;

use super::helpers::parse_index_method;
use super::LuaIndexColumn;

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
        // column(name) -> self
        methods.add_method("column", |_, this, name: String| {
            this.transform(|builder| builder.column(name));
            Ok(this.clone())
        });

        // columns(names) -> self
        methods.add_method("columns", |_, this, names: Vec<String>| {
            this.transform(|builder| builder.columns(names));
            Ok(this.clone())
        });

        // expression(expr) -> self
        methods.add_method("expression", |_, this, expr: String| {
            this.transform(|builder| builder.expression(expr));
            Ok(this.clone())
        });

        // index_column(lua_index_column) -> self
        methods.add_method("index_column", |_, this, col: LuaIndexColumn| {
            this.transform(|builder| builder.index_column(col.into_inner()));
            Ok(this.clone())
        });

        // unique() -> self
        methods.add_method("unique", |_, this, ()| {
            this.transform(|builder| builder.unique());
            Ok(this.clone())
        });

        // using(method) -> self
        methods.add_method("using", |_, this, method: String| {
            this.transform(|builder| builder.using(parse_index_method(&method)));
            Ok(this.clone())
        });

        // where_clause(clause) -> self
        methods.add_method("where_clause", |_, this, clause: String| {
            this.transform(|builder| builder.where_clause(clause));
            Ok(this.clone())
        });

        // include(columns) -> self
        methods.add_method("include", |_, this, columns: Vec<String>| {
            this.transform(|builder| builder.include(columns));
            Ok(this.clone())
        });

        // concurrently() -> self
        methods.add_method("concurrently", |_, this, ()| {
            this.transform(|builder| builder.concurrently());
            Ok(this.clone())
        });

        // tablespace(name) -> self
        methods.add_method("tablespace", |_, this, tablespace: String| {
            this.transform(|builder| builder.tablespace(tablespace));
            Ok(this.clone())
        });

        // option(key, value) -> self
        methods.add_method("option", |_, this, (key, value): (String, String)| {
            this.transform(|builder| builder.option(key, value));
            Ok(this.clone())
        });
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
