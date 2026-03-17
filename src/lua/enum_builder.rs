//! Lua wrapper for EnumBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{EnumBuilder, EnumType};

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
        // value(val) -> self
        methods.add_method("value", |_, this, value: String| {
            this.transform(|builder| builder.value(value));
            Ok(this.clone())
        });

        // values(vals) -> self
        methods.add_method("values", |_, this, values: Vec<String>| {
            this.transform(|builder| builder.values(values));
            Ok(this.clone())
        });

        // description(desc) -> self
        methods.add_method("description", |_, this, desc: String| {
            this.transform(|builder| builder.description(desc));
            Ok(this.clone())
        });
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
