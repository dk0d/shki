//! Lua wrapper for SequenceBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{Sequence, SequenceBuilder};

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
        // schema(name) -> self
        methods.add_method("schema", |_, this, schema: String| {
            this.transform(|builder| builder.schema(schema));
            Ok(this.clone())
        });

        // increment(value) -> self
        methods.add_method("increment", |_, this, increment: i64| {
            this.transform(|builder| builder.increment(increment));
            Ok(this.clone())
        });

        // min_value(value) -> self
        methods.add_method("min_value", |_, this, min_value: i64| {
            this.transform(|builder| builder.min_value(min_value));
            Ok(this.clone())
        });

        // max_value(value) -> self
        methods.add_method("max_value", |_, this, max_value: i64| {
            this.transform(|builder| builder.max_value(max_value));
            Ok(this.clone())
        });

        // start(value) -> self
        methods.add_method("start", |_, this, start: i64| {
            this.transform(|builder| builder.start(start));
            Ok(this.clone())
        });

        // cache(value) -> self
        methods.add_method("cache", |_, this, cache: i64| {
            this.transform(|builder| builder.cache(cache));
            Ok(this.clone())
        });

        // cycle() -> self
        methods.add_method("cycle", |_, this, ()| {
            this.transform(|builder| builder.cycle());
            Ok(this.clone())
        });

        // no_cycle() -> self
        methods.add_method("no_cycle", |_, this, ()| {
            this.transform(|builder| builder.no_cycle());
            Ok(this.clone())
        });
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
