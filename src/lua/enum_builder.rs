//! Lua wrapper for EnumBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{EnumBuilder, EnumType};

crate::lua_global_module! {
    metadata: ENUM_BUILDER_LUA_MODULE,
    register: register_enum_builder_module,
    name: "EnumBuilder",
    doc: "Builder for enum types.",
    functions: [
        fn "new"(name: String => ("string", "string", "Name")) -> "EnumBuilder" => enum_builder_new;
    ],
}

crate::lua_builder_def! {
    target: LuaEnumBuilder,
    metadata: ENUM_BUILDER_LUA_TYPE,
    register: register_lua_enum_builder_methods,
    type_name: "EnumBuilder",
    doc: "Builder for enum types.",
    fields: [],
    methods: [
        method "value"(value: String => ("string", "string", "Enum value")) -> "EnumBuilder" => |this, value| { this.transform(|builder| builder.value(value)); Ok(this.clone()) };
        method "values"(values: Vec<String> => ("string[]", "table", "Array of enum values")) -> "EnumBuilder" => |this, values| { this.transform(|builder| builder.values(values)); Ok(this.clone()) };
        method "description"(desc: String => ("string", "string", "Description text")) -> "EnumBuilder" => |this, desc| { this.transform(|builder| builder.description(desc)); Ok(this.clone()) };
    ],
}

/// Lua wrapper for EnumBuilder
#[derive(Clone)]
pub struct LuaEnumBuilder {
    inner: Rc<RefCell<EnumBuilder>>,
}

impl LuaEnumBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnumBuilder::new(name))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(EnumBuilder) -> EnumBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> EnumType {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }

    pub fn enum_type(&self) -> EnumType {
        self.inner.borrow().clone().build()
    }
}

impl UserData for LuaEnumBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_enum_builder_methods(methods);
    }
}

impl FromLua for LuaEnumBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaEnumBuilder".to_string(),
                message: Some("expected EnumBuilder".to_string()),
            }),
        }
    }
}

/// EnumBuilder.new(name) -> LuaEnumBuilder
pub fn enum_builder_new(_: &Lua, name: String) -> LuaResult<LuaEnumBuilder> {
    Ok(LuaEnumBuilder::new(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_enum_builder_metadata_matches_runtime_api() {
        assert_eq!(ENUM_BUILDER_LUA_MODULE.name, "EnumBuilder");
        assert!(ENUM_BUILDER_LUA_MODULE.global);
        assert_eq!(ENUM_BUILDER_LUA_MODULE.methods[0].name, "new");
        assert!(ENUM_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "values"));
        assert!(ENUM_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "description"));
    }
}
