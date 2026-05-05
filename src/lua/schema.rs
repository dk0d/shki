//! Lua wrapper for Schema

use mlua::{FromLua, Lua, MetaMethod, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::Schema;

use super::{LuaEnumBuilder, LuaSequenceBuilder, LuaTableBuilder, LuaViewBuilder};

crate::lua_global_module! {
    metadata: PG_LUA_MODULE,
    register: register_pg_module,
    name: "pg",
    doc: "PostgreSQL module.",
    functions: [
        fn "schema"(name: String => ("string", "string", "Schema name")) -> "Schema" => pg_schema;
    ],
}

crate::lua_global_module! {
    metadata: MYSQL_LUA_MODULE,
    register: register_mysql_module,
    name: "mysql",
    doc: "MySQL module.",
    functions: [
        fn "schema"(name: String => ("string", "string", "Database name")) -> "Schema" => mysql_schema;
    ],
}

crate::lua_global_module! {
    metadata: SQLITE_LUA_MODULE,
    register: register_sqlite_module,
    name: "sqlite",
    doc: "SQLite module.",
    functions: [
        fn "schema"() -> "Schema" => sqlite_schema;
    ],
}

crate::lua_builder_def! {
    target: LuaSchema,
    metadata: SCHEMA_LUA_TYPE,
    register: register_lua_schema_methods,
    type_name: "Schema",
    doc: "Schema object returned by dialect modules.",
    fields: [
        field name: "string" => "Schema name.";
        field dialect: "string" => "Database dialect.";
    ],
    methods: [
        method "table"(table: LuaTableBuilder => ("TableBuilder", "any", "Table builder")) -> "Schema" => |this, table| {
            let table = table.build();
            this.inner.borrow_mut().table(table);
            Ok(this.clone())
        };
        method "enum"(enum_type: LuaEnumBuilder => ("EnumBuilder", "any", "Enum builder")) -> "Schema" => |this, enum_type| {
            let enum_type = enum_type.build();
            this.inner.borrow_mut().enum_type(enum_type);
            Ok(this.clone())
        };
        method "sequence"(sequence: LuaSequenceBuilder => ("SequenceBuilder", "any", "Sequence builder")) -> "Schema" => |this, sequence| {
            let sequence = sequence.build();
            this.inner.borrow_mut().sequence(sequence);
            Ok(this.clone())
        };
        method "view"(view: LuaViewBuilder => ("ViewBuilder", "any", "View builder")) -> "Schema" => |this, view| {
            let view = view.build();
            this.inner.borrow_mut().view(view);
            Ok(this.clone())
        };
        method "extension"(name: String => ("string", "string", "Extension name")) -> "Schema" => |this, name| {
            this.inner.borrow_mut().extension(name);
            Ok(this.clone())
        };
    ],
}

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
        register_lua_schema_methods(methods);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_schema_metadata_matches_runtime_api() {
        assert_eq!(SCHEMA_LUA_TYPE.name, "Schema");
        assert!(
            SCHEMA_LUA_TYPE
                .methods
                .iter()
                .any(|method| method.name == "enum")
        );
        assert!(
            SCHEMA_LUA_TYPE
                .methods
                .iter()
                .any(|method| method.name == "sequence")
        );
        assert!(
            SCHEMA_LUA_TYPE
                .fields
                .iter()
                .any(|field| field.name == "dialect")
        );
    }

    #[test]
    fn macro_generated_module_metadata_matches_runtime_api() {
        assert_eq!(PG_LUA_MODULE.name, "pg");
        assert_eq!(MYSQL_LUA_MODULE.name, "mysql");
        assert_eq!(SQLITE_LUA_MODULE.name, "sqlite");
        assert_eq!(PG_LUA_MODULE.methods[0].name, "schema");
    }
}
