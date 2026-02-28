//! Lua wrapper for ViewBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{View, ViewBuilder};

/// Lua wrapper for building Views
#[derive(Clone)]
pub struct LuaViewBuilder {
    inner: Rc<RefCell<ViewBuilder>>,
}

impl LuaViewBuilder {
    pub fn new(name: String, definition: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ViewBuilder::new(name, definition))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(ViewBuilder) -> ViewBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> View {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaViewBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // schema(name) -> self
        methods.add_method("schema", |_, this, schema: String| {
            this.transform(|builder| builder.schema(schema));
            Ok(this.clone())
        });

        // materialized() -> self
        methods.add_method("materialized", |_, this, ()| {
            this.transform(|builder| builder.materialized());
            Ok(this.clone())
        });
    }
}

impl FromLua for LuaViewBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaViewBuilder".to_string(),
                message: Some("expected ViewBuilder".to_string()),
            }),
        }
    }
}

/// ViewBuilder.new(name, definition) -> LuaViewBuilder
pub fn view_builder_new(
    _: &Lua,
    (name, definition): (String, String),
) -> LuaResult<LuaViewBuilder> {
    Ok(LuaViewBuilder::new(name, definition))
}
