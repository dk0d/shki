//! Lua wrapper for ColumnBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::types::{DataType, DefaultValue};
use crate::schema::{Column, ColumnBuilder};

use super::LuaEnumBuilder;
use super::helpers::{parse_data_type, parse_referential_action};

crate::lua_global_module! {
    metadata: COLUMN_BUILDER_LUA_MODULE,
    register: register_column_builder_module,
    name: "ColumnBuilder",
    doc: "Builder for columns.",
    functions: [
        fn "new"(name: (String, String) => ("string", "string", "Column name"), type_name: (String, String) => ("string", "string", "SQL type name")) -> "ColumnBuilder" => column_builder_new;
        fn "serial"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_serial;
        fn "bigserial"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_bigserial;
        fn "smallserial"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_smallserial;
        fn "integer"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_integer;
        fn "bigint"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_bigint;
        fn "smallint"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_smallint;
        fn "text"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_text;
        fn "varchar"(name: (String, Option<u32>) => ("string", "string", "Column name"), length: (String, Option<u32>) => ("integer", "number", "Length", optional)) -> "ColumnBuilder" => column_builder_varchar;
        fn "char"(name: (String, Option<u32>) => ("string", "string", "Column name"), length: (String, Option<u32>) => ("integer", "number", "Length", optional)) -> "ColumnBuilder" => column_builder_char;
        fn "boolean"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_boolean;
        fn "timestamp"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_timestamp;
        fn "timestamptz"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_timestamptz;
        fn "date"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_date;
        fn "time"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_time;
        fn "uuid"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_uuid;
        fn "json"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_json;
        fn "jsonb"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_jsonb;
        fn "numeric"(name: (String, Option<u32>, Option<u32>) => ("string", "string", "Column name"), precision: (String, Option<u32>, Option<u32>) => ("integer", "number", "Total digits", optional), scale: (String, Option<u32>, Option<u32>) => ("integer", "number", "Decimal places", optional)) -> "ColumnBuilder" => column_builder_numeric;
        fn "real"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_real;
        fn "double_precision"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_double_precision;
        fn "bytea"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_bytea;
        fn "inet"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_inet;
        fn "cidr"(name: String => ("string", "string", "Column name")) -> "ColumnBuilder" => column_builder_cidr;
        fn "enum"(name: (String, LuaEnumTypeInput) => ("string", "string", "Column name"), enum_name: (String, LuaEnumTypeInput) => ("string|EnumBuilder", "any", "Enum name or builder")) -> "ColumnBuilder" => column_builder_enum_type;
        fn "array"(name: (String, String) => ("string", "string", "Column name"), element_type: (String, String) => ("string", "string", "Element type name")) -> "ColumnBuilder" => column_builder_array;
    ],
}

crate::lua_builder_def! {
    target: LuaColumnBuilder,
    metadata: COLUMN_BUILDER_LUA_TYPE,
    register: register_lua_column_builder_methods,
    type_name: "ColumnBuilder",
    doc: "Builder for columns.",
    fields: [],
    methods: [
        method "not_null"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.not_null()); Ok(this.clone()) };
        method "nullable"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.nullable()); Ok(this.clone()) };
        method "primary_key"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.primary_key()); Ok(this.clone()) };
        method "unique"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.unique()); Ok(this.clone()) };
        method "default_value"(value: String => ("string", "string", "Default SQL literal")) -> "ColumnBuilder" => |this, value| { this.transform(|builder| builder.default_value(value)); Ok(this.clone()) };
        method "default_now"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default_now()); Ok(this.clone()) };
        method "default_sql"(expr: String => ("string", "string", "SQL expression")) -> "ColumnBuilder" => |this, expr| { this.transform(|builder| builder.default(DefaultValue::Sql(expr))); Ok(this.clone()) };
        method "default_null"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::Literal("NULL".to_string()))); Ok(this.clone()) };
        method "default_current_timestamp"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::current_timestamp())); Ok(this.clone()) };
        method "default_uuid_generate_v4"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::uuid_generate_v4())); Ok(this.clone()) };
        method "default_gen_random_uuid"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::gen_random_uuid())); Ok(this.clone()) };
        method "default_uuidv7"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::uuidv7())); Ok(this.clone()) };
        method "default_uuidv4"() -> "ColumnBuilder" => |this| { this.transform(|builder| builder.default(DefaultValue::uuidv4())); Ok(this.clone()) };
        method "description"(desc: String => ("string", "string", "Description text")) -> "ColumnBuilder" => |this, desc| { this.transform(|builder| builder.description(desc)); Ok(this.clone()) };
        method "comment"(comment: String => ("string", "string", "Comment text")) -> "ColumnBuilder" => |this, comment| { this.transform(|builder| builder.comment(comment)); Ok(this.clone()) };
        method "collate"(collation: String => ("string", "string", "Collation name")) -> "ColumnBuilder" => |this, collation| { this.transform(|builder| builder.collate(collation)); Ok(this.clone()) };
        method "references"(table: String => ("string", "string", "Referenced table"), column: String => ("string", "string", "Referenced column")) -> "ColumnBuilder" => |this, table, column| { this.transform(|builder| builder.references(table, column)); Ok(this.clone()) };
        method "references_on_delete"(table: String => ("string", "string", "Referenced table"), column: String => ("string", "string", "Referenced column"), action: String => ("ReferenceAction|string", "string", "ON DELETE action")) -> "ColumnBuilder" => |this, table, column, action| { let on_delete = parse_referential_action(&action); this.transform(|builder| builder.references_on_delete(table, column, on_delete)); Ok(this.clone()) };
        method "identity"(always: bool => ("boolean", "bool", "Use ALWAYS when true")) -> "ColumnBuilder" => |this, always| { this.transform(|builder| builder.identity(always)); Ok(this.clone()) };
        method "generated_as"(expression: String => ("string", "string", "Generated expression"), stored: bool => ("boolean", "bool", "Store generated values")) -> "ColumnBuilder" => |this, expression, stored| { this.transform(|builder| builder.generated_as(expression, stored)); Ok(this.clone()) };
    ],
}

pub(crate) enum LuaEnumTypeInput {
    Name(String),
    Builder(LuaEnumBuilder),
}

impl FromLua for LuaEnumTypeInput {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(s) => Ok(Self::Name(s.to_str()?.to_string())),
            other => LuaEnumBuilder::from_lua(other, lua).map(Self::Builder),
        }
    }
}

/// Lua wrapper for ColumnBuilder
#[derive(Clone)]
pub struct LuaColumnBuilder {
    inner: Rc<RefCell<ColumnBuilder>>,
}

impl LuaColumnBuilder {
    pub fn new(name: String, data_type: DataType) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::new(name, data_type))),
        }
    }

    pub fn from_type_name(name: String, type_name: &str) -> Self {
        let data_type = parse_data_type(type_name);
        Self::new(name, data_type)
    }

    pub fn serial(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::serial(name))),
        }
    }

    pub fn bigserial(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::bigserial(name))),
        }
    }

    pub fn smallserial(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::smallserial(name))),
        }
    }

    pub fn integer(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::integer(name))),
        }
    }

    pub fn bigint(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::bigint(name))),
        }
    }

    pub fn smallint(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::smallint(name))),
        }
    }

    pub fn text(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::text(name))),
        }
    }

    pub fn varchar(name: String, length: Option<u32>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::varchar(name, length))),
        }
    }

    pub fn char(name: String, length: Option<u32>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::char(name, length))),
        }
    }

    pub fn boolean(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::boolean(name))),
        }
    }

    pub fn timestamp(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::timestamp(name))),
        }
    }

    pub fn timestamptz(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::timestamptz(name))),
        }
    }

    pub fn date(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::date(name))),
        }
    }

    pub fn time(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::time(name))),
        }
    }

    pub fn uuid(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::uuid(name))),
        }
    }

    pub fn json(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::json(name))),
        }
    }

    pub fn jsonb(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::jsonb(name))),
        }
    }

    pub fn numeric(name: String, precision: Option<u32>, scale: Option<u32>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::numeric(name, precision, scale))),
        }
    }

    pub fn real(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::real(name))),
        }
    }

    pub fn double_precision(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::double_precision(name))),
        }
    }

    pub fn bytea(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::bytea(name))),
        }
    }

    pub fn inet(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::inet(name))),
        }
    }

    pub fn cidr(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::cidr(name))),
        }
    }

    pub fn enum_type(name: String, enum_name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::enum_type(name, enum_name))),
        }
    }

    pub fn enum_type_from_builder(name: String, enum_builder: LuaEnumBuilder) -> Self {
        let enum_type = enum_builder.enum_type();
        Self::new(
            name,
            DataType::Enum {
                name: enum_type.name,
                schema: enum_type.schema,
            },
        )
    }

    pub fn array(name: String, element_type: String) -> Self {
        let inner_type = parse_data_type(&element_type);
        Self {
            inner: Rc::new(RefCell::new(ColumnBuilder::array(name, inner_type))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(ColumnBuilder) -> ColumnBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> Column {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaColumnBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        register_lua_column_builder_methods(methods);
    }
}

impl FromLua for LuaColumnBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaColumnBuilder".to_string(),
                message: Some("expected ColumnBuilder".to_string()),
            }),
        }
    }
}

// ============================================================================
// ColumnBuilder constructors
// ============================================================================

pub fn column_builder_new(
    _lua: &Lua,
    (name, type_name): (String, String),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::from_type_name(name, &type_name))
}

pub fn column_builder_serial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::serial(name))
}

pub fn column_builder_bigserial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::bigserial(name))
}

pub fn column_builder_smallserial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::smallserial(name))
}

pub fn column_builder_integer(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::integer(name))
}

pub fn column_builder_bigint(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::bigint(name))
}

pub fn column_builder_smallint(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::smallint(name))
}

pub fn column_builder_text(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::text(name))
}

pub fn column_builder_varchar(
    _lua: &Lua,
    (name, length): (String, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::varchar(name, length))
}

pub fn column_builder_char(
    _lua: &Lua,
    (name, length): (String, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::char(name, length))
}

pub fn column_builder_boolean(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::boolean(name))
}

pub fn column_builder_timestamp(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::timestamp(name))
}

pub fn column_builder_timestamptz(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::timestamptz(name))
}

pub fn column_builder_date(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::date(name))
}

pub fn column_builder_time(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::time(name))
}

pub fn column_builder_uuid(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::uuid(name))
}

pub fn column_builder_json(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::json(name))
}

pub fn column_builder_jsonb(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::jsonb(name))
}

pub fn column_builder_numeric(
    _lua: &Lua,
    (name, precision, scale): (String, Option<u32>, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::numeric(name, precision, scale))
}

pub fn column_builder_real(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::real(name))
}

pub fn column_builder_double_precision(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::double_precision(name))
}

pub fn column_builder_bytea(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::bytea(name))
}

pub fn column_builder_inet(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::inet(name))
}

pub fn column_builder_cidr(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::cidr(name))
}

pub(crate) fn column_builder_enum_type(
    _lua: &Lua,
    (name, enum_input): (String, LuaEnumTypeInput),
) -> LuaResult<LuaColumnBuilder> {
    Ok(match enum_input {
        LuaEnumTypeInput::Name(enum_name) => LuaColumnBuilder::enum_type(name, enum_name),
        LuaEnumTypeInput::Builder(enum_builder) => {
            LuaColumnBuilder::enum_type_from_builder(name, enum_builder)
        }
    })
}

pub fn column_builder_array(
    _lua: &Lua,
    (name, element_type): (String, String),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::array(name, element_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_generated_column_builder_metadata_matches_runtime_api() {
        assert_eq!(COLUMN_BUILDER_LUA_MODULE.name, "ColumnBuilder");
        assert!(COLUMN_BUILDER_LUA_MODULE.global);
        assert!(
            COLUMN_BUILDER_LUA_MODULE
                .methods
                .iter()
                .any(|method| method.name == "enum")
        );
        assert!(
            COLUMN_BUILDER_LUA_TYPE
                .methods
                .iter()
                .any(|method| method.name == "generated_as")
        );
        assert!(
            COLUMN_BUILDER_LUA_MODULE
                .methods
                .iter()
                .find(|method| method.name == "varchar")
                .unwrap()
                .params[1]
                .optional
        );
        assert!(
            COLUMN_BUILDER_LUA_MODULE
                .methods
                .iter()
                .find(|method| method.name == "enum")
                .unwrap()
                .params[1]
                .luacats_type
                .contains("EnumBuilder")
        );
    }
}
