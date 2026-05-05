//! Lua wrapper for SequenceBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{Sequence, SequenceBuilder};

crate::lua_global_module! {
    metadata: SEQUENCE_BUILDER_LUA_MODULE,
    register: register_sequence_builder_module,
    name: "SequenceBuilder",
    doc: "Builder for sequences.",
    functions: [
        fn "new"(name: String => ("string", "string", "Name")) -> "SequenceBuilder" => sequence_builder_new;
    ],
}

crate::lua_builder_def! {
    target: LuaSequenceBuilder,
    metadata: SEQUENCE_BUILDER_LUA_TYPE,
    register: register_lua_sequence_builder_methods,
    type_name: "SequenceBuilder",
    doc: "Builder for sequences.",
    fields: [],
    methods: [
        method "schema"(schema: String => ("string", "string", "Schema name")) -> "SequenceBuilder" => |this, schema| { this.transform(|builder| builder.schema(schema)); Ok(this.clone()) };
        method "increment"(increment: i64 => ("integer", "number", "Sequence value")) -> "SequenceBuilder" => |this, increment| { this.transform(|builder| builder.increment(increment)); Ok(this.clone()) };
        method "min_value"(min_value: i64 => ("integer", "number", "Sequence value")) -> "SequenceBuilder" => |this, min_value| { this.transform(|builder| builder.min_value(min_value)); Ok(this.clone()) };
        method "max_value"(max_value: i64 => ("integer", "number", "Sequence value")) -> "SequenceBuilder" => |this, max_value| { this.transform(|builder| builder.max_value(max_value)); Ok(this.clone()) };
        method "start"(start: i64 => ("integer", "number", "Sequence value")) -> "SequenceBuilder" => |this, start| { this.transform(|builder| builder.start(start)); Ok(this.clone()) };
        method "cache"(cache: i64 => ("integer", "number", "Sequence value")) -> "SequenceBuilder" => |this, cache| { this.transform(|builder| builder.cache(cache)); Ok(this.clone()) };
        method "cycle"() -> "SequenceBuilder" => |this| { this.transform(|builder| builder.cycle()); Ok(this.clone()) };
        method "no_cycle"() -> "SequenceBuilder" => |this| { this.transform(|builder| builder.no_cycle()); Ok(this.clone()) };
    ],
}

/// Lua wrapper for building Sequences
#[derive(Clone)]
pub struct LuaSequenceBuilder {
    inner: Rc<RefCell<SequenceBuilder>>,
}

impl LuaSequenceBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(SequenceBuilder::new(name))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(SequenceBuilder) -> SequenceBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> Sequence {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaSequenceBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_sequence_builder_methods(methods);
    }
}

impl FromLua for LuaSequenceBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaSequenceBuilder".to_string(),
                message: Some("expected SequenceBuilder".to_string()),
            }),
        }
    }
}

/// SequenceBuilder.new(name) -> LuaSequenceBuilder
pub fn sequence_builder_new(_: &Lua, name: String) -> LuaResult<LuaSequenceBuilder> {
    Ok(LuaSequenceBuilder::new(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_sequence_builder_metadata_matches_runtime_api() {
        assert_eq!(SEQUENCE_BUILDER_LUA_MODULE.name, "SequenceBuilder");
        assert!(SEQUENCE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "schema"));
        assert!(SEQUENCE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "cycle"));
        assert!(SEQUENCE_BUILDER_LUA_TYPE
            .methods
            .iter()
            .any(|m| m.name == "no_cycle"));
    }
}
