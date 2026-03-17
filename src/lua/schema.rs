//! Lua wrapper for Schema

use mlua::{FromLua, Lua, MetaMethod, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::Schema;

use super::{LuaEnumBuilder, LuaSequenceBuilder, LuaTableBuilder, LuaViewBuilder};

/// Lua wrapper for Schema
#[derive(Clone)]
pub struct LuaSchema {
    inner: Rc<RefCell<Schema>>,
}

impl LuaSchema {
    pub fn new(schema: Schema) -> Self {
        Self {
            inner: Rc::new(RefCell::new(schema)),
        }
    }

    pub fn postgres(name: String) -> Self {
        Self::new(Schema::postgres(name))
    }

    pub fn mysql(name: String) -> Self {
        Self::new(Schema::mysql(name))
    }

    pub fn sqlite() -> Self {
        Self::new(Schema::sqlite())
    }

    pub fn into_schema(self) -> Schema {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner())
            .unwrap_or_else(|rc| rc.borrow().clone())
    }
}

impl UserData for LuaSchema {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // table(table_builder) -> self
        methods.add_method("table", |_, this, table: LuaTableBuilder| {
            let table = table.build();
            this.inner.borrow_mut().table(table);
            Ok(this.clone())
        });

        // enum(enum_builder) -> self
        methods.add_method("enum", |_, this, enum_type: LuaEnumBuilder| {
            let enum_type = enum_type.build();
            this.inner.borrow_mut().enum_type(enum_type);
            Ok(this.clone())
        });

        // sequence(sequence_builder) -> self
        methods.add_method("sequence", |_, this, sequence: LuaSequenceBuilder| {
            let sequence = sequence.build();
            this.inner.borrow_mut().sequence(sequence);
            Ok(this.clone())
        });

        // view(view_builder) -> self
        methods.add_method("view", |_, this, view: LuaViewBuilder| {
            let view = view.build();
            this.inner.borrow_mut().view(view);
            Ok(this.clone())
        });

        // extension(name) -> self
        methods.add_method("extension", |_, this, name: String| {
            this.inner.borrow_mut().extension(name);
            Ok(this.clone())
        });

        // Getter for name
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            match key.as_str() {
                "name" => Ok(Some(this.inner.borrow().name.clone())),
                "dialect" => Ok(Some(format!("{:?}", this.inner.borrow().dialect))),
                _ => Ok(None),
            }
        });
    }
}

impl FromLua for LuaSchema {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaSchema".to_string(),
                message: Some("expected Schema".to_string()),
            }),
        }
    }
}

/// pg.schema(name) -> LuaSchema
pub fn pg_schema(_: &Lua, name: String) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::postgres(name))
}

/// mysql.schema(name) -> LuaSchema
pub fn mysql_schema(_: &Lua, name: String) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::mysql(name))
}

/// sqlite.schema() -> LuaSchema
pub fn sqlite_schema(_: &Lua, _: ()) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::sqlite())
}
