//! Lua wrapper for ViewBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};

use crate::schema::View;

/// Lua wrapper for building Views
#[derive(Clone)]
pub struct LuaViewBuilder {
    name: String,
    schema: Option<String>,
    definition: String,
    materialized: bool,
}

impl LuaViewBuilder {
    pub fn new(name: String, definition: String) -> Self {
        Self {
            name,
            schema: None,
            definition,
            materialized: false,
        }
    }

    pub fn build(self) -> View {
        View {
            name: self.name,
            schema: self.schema,
            definition: self.definition,
            materialized: self.materialized,
            columns: Vec::new(), // Columns are typically inferred from the definition
        }
    }
}

impl UserData for LuaViewBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // schema(name) -> self
        methods.add_method("schema", |_, this, schema: String| {
            let mut new = this.clone();
            new.schema = Some(schema);
            Ok(new)
        });

        // materialized() -> self
        methods.add_method("materialized", |_, this, ()| {
            let mut new = this.clone();
            new.materialized = true;
            Ok(new)
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
    _lua: &Lua,
    (name, definition): (String, String),
) -> LuaResult<LuaViewBuilder> {
    Ok(LuaViewBuilder::new(name, definition))
}
