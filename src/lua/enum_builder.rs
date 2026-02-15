//! Lua wrapper for EnumBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};

use crate::schema::{EnumBuilder, EnumType};

/// Lua wrapper for EnumBuilder
#[derive(Clone)]
pub struct LuaEnumBuilder {
    name: String,
    values: Vec<String>,
    description: Option<String>,
}

impl LuaEnumBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            values: Vec::new(),
            description: None,
        }
    }

    pub fn build(self) -> EnumType {
        let mut builder = EnumBuilder::new(self.name);
        for value in self.values {
            builder = builder.value(value);
        }
        if let Some(desc) = self.description {
            builder = builder.description(desc);
        }
        builder.build()
    }
}

impl UserData for LuaEnumBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // value(val) -> self
        methods.add_method("value", |_, this, value: String| {
            let mut new = this.clone();
            new.values.push(value);
            Ok(new)
        });

        // values(vals) -> self
        methods.add_method("values", |_, this, values: Vec<String>| {
            let mut new = this.clone();
            new.values.extend(values);
            Ok(new)
        });

        // description(desc) -> self
        methods.add_method("description", |_, this, desc: String| {
            let mut new = this.clone();
            new.description = Some(desc);
            Ok(new)
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
pub fn enum_builder_new(_lua: &Lua, name: String) -> LuaResult<LuaEnumBuilder> {
    Ok(LuaEnumBuilder::new(name))
}
