//! Lua wrapper for ViewBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{View, ViewBuilder};

crate::lua_global_module! {
    metadata: VIEW_BUILDER_LUA_MODULE,
    register: register_view_builder_module,
    name: "ViewBuilder",
    doc: "Builder for views.",
    functions: [
        fn "new"(name: (String, String) => ("string", "string", "View name"), definition: (String, String) => ("string", "string", "SQL definition")) -> "ViewBuilder" => view_builder_new;
    ],
}

crate::lua_builder_def! {
    target: LuaViewBuilder,
    metadata: VIEW_BUILDER_LUA_TYPE,
    register: register_lua_view_builder_methods,
    type_name: "ViewBuilder",
    doc: "Builder for views.",
    fields: [],
    methods: [
        method "schema"(schema: String => ("string", "string", "Schema name")) -> "ViewBuilder" => |this, schema| { this.transform(|builder| builder.schema(schema)); Ok(this.clone()) };
        method "materialized"() -> "ViewBuilder" => |this| { this.transform(|builder| builder.materialized()); Ok(this.clone()) };
    ],
}

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
        register_lua_view_builder_methods(methods);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_view_builder_metadata_matches_runtime_api() {
        assert_eq!(VIEW_BUILDER_LUA_MODULE.name, "ViewBuilder");
        assert!(VIEW_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "schema"));
        assert!(VIEW_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "materialized"));
    }
}
