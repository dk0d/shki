//! Lua wrapper for ColumnBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::Column;
use crate::schema::types::{DataType, DefaultValue, ReferenceAction};

use super::helpers::{parse_data_type, parse_referential_action};

/// Lua wrapper for ColumnBuilder
#[derive(Clone)]
pub struct LuaColumnBuilder {
    inner: Rc<RefCell<Column>>,
}

impl LuaColumnBuilder {
    pub fn new(name: String, data_type: DataType) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Column::new(name, data_type))),
        }
    }

    pub fn build(self) -> Column {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner())
            .unwrap_or_else(|rc| rc.borrow().clone())
    }
}

impl UserData for LuaColumnBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // not_null() -> self
        methods.add_method("not_null", |_, this, ()| {
            this.inner.borrow_mut().nullable = false;
            Ok(this.clone())
        });

        // nullable() -> self
        methods.add_method("nullable", |_, this, ()| {
            this.inner.borrow_mut().nullable = true;
            Ok(this.clone())
        });

        // primary_key() -> self
        methods.add_method("primary_key", |_, this, ()| {
            this.inner.borrow_mut().primary_key = true;
            this.inner.borrow_mut().nullable = false;
            Ok(this.clone())
        });

        // unique() -> self
        methods.add_method("unique", |_, this, ()| {
            this.inner.borrow_mut().unique = true;
            Ok(this.clone())
        });

        // default_value(value) -> self
        methods.add_method("default_value", |_, this, value: String| {
            this.inner.borrow_mut().default = Some(DefaultValue::Literal(value));
            Ok(this.clone())
        });

        // default_now() -> self
        methods.add_method("default_now", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::now());
            Ok(this.clone())
        });

        // default_sql(expr) -> self
        methods.add_method("default_sql", |_, this, expr: String| {
            this.inner.borrow_mut().default = Some(DefaultValue::Sql(expr));
            Ok(this.clone())
        });

        // default_null() -> self
        methods.add_method("default_null", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::Literal("NULL".to_string()));
            Ok(this.clone())
        });

        // default_current_timestamp() -> self
        methods.add_method("default_current_timestamp", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::current_timestamp());
            Ok(this.clone())
        });

        // default_uuid_generate_v4() -> self
        methods.add_method("default_uuid_generate_v4", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::uuid_generate_v4());
            Ok(this.clone())
        });

        // default_gen_random_uuid() -> self
        methods.add_method("default_gen_random_uuid", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::gen_random_uuid());
            Ok(this.clone())
        });

        // default_uuidv7() -> self
        methods.add_method("default_uuidv7", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::uuidv7());
            Ok(this.clone())
        });

        // default_uuidv4() -> self
        methods.add_method("default_uuidv4", |_, this, ()| {
            this.inner.borrow_mut().default = Some(DefaultValue::uuidv4());
            Ok(this.clone())
        });

        // description(desc) -> self (alias for comment)
        methods.add_method("description", |_, this, desc: String| {
            this.inner.borrow_mut().comment = Some(desc);
            Ok(this.clone())
        });

        // comment(text) -> self
        methods.add_method("comment", |_, this, comment: String| {
            this.inner.borrow_mut().comment = Some(comment);
            Ok(this.clone())
        });

        // collate(collation) -> self
        methods.add_method("collate", |_, this, collation: String| {
            this.inner.borrow_mut().collation = Some(collation);
            Ok(this.clone())
        });

        // references(table, column) -> self
        methods.add_method(
            "references",
            |_, this, (table, column): (String, String)| {
                use crate::schema::column::ColumnReference;
                this.inner.borrow_mut().references = Some(ColumnReference {
                    table,
                    column,
                    on_delete: ReferenceAction::NoAction,
                    on_update: ReferenceAction::NoAction,
                });
                Ok(this.clone())
            },
        );

        // references_on_delete(table, column, action) -> self
        methods.add_method(
            "references_on_delete",
            |_, this, (table, column, action): (String, String, String)| {
                use crate::schema::column::ColumnReference;
                let on_delete = parse_referential_action(&action);
                this.inner.borrow_mut().references = Some(ColumnReference {
                    table,
                    column,
                    on_delete,
                    on_update: ReferenceAction::NoAction,
                });
                Ok(this.clone())
            },
        );

        // identity(always) -> self
        methods.add_method("identity", |_, this, always: bool| {
            use crate::schema::column::IdentitySpec;
            this.inner.borrow_mut().identity = Some(IdentitySpec {
                always,
                sequence_options: None,
            });
            Ok(this.clone())
        });

        // generated_as(expression, stored) -> self
        methods.add_method(
            "generated_as",
            |_, this, (expression, stored): (String, bool)| {
                use crate::schema::types::GeneratedColumn;
                this.inner.borrow_mut().generated = Some(GeneratedColumn { expression, stored });
                Ok(this.clone())
            },
        );
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
    let data_type = parse_data_type(&type_name);
    Ok(LuaColumnBuilder::new(name, data_type))
}

pub fn column_builder_serial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Serial))
}

pub fn column_builder_bigserial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::BigSerial))
}

pub fn column_builder_smallserial(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::SmallSerial))
}

pub fn column_builder_integer(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Integer))
}

pub fn column_builder_bigint(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::BigInt))
}

pub fn column_builder_smallint(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::SmallInt))
}

pub fn column_builder_text(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Text))
}

pub fn column_builder_varchar(
    _lua: &Lua,
    (name, length): (String, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::VarChar { length }))
}

pub fn column_builder_char(
    _lua: &Lua,
    (name, length): (String, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Char { length }))
}

pub fn column_builder_boolean(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Boolean))
}

pub fn column_builder_timestamp(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Timestamp {
            precision: None,
            with_timezone: false,
        },
    ))
}

pub fn column_builder_timestamptz(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Timestamp {
            precision: None,
            with_timezone: true,
        },
    ))
}

pub fn column_builder_date(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Date))
}

pub fn column_builder_time(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Time {
            precision: None,
            with_timezone: false,
        },
    ))
}

pub fn column_builder_uuid(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Uuid))
}

pub fn column_builder_json(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Json))
}

pub fn column_builder_jsonb(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::JsonB))
}

pub fn column_builder_numeric(
    _lua: &Lua,
    (name, precision, scale): (String, Option<u32>, Option<u32>),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Numeric { precision, scale },
    ))
}

pub fn column_builder_real(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Real))
}

pub fn column_builder_double_precision(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::DoublePrecision))
}

pub fn column_builder_bytea(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::ByteA))
}

pub fn column_builder_inet(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Inet))
}

pub fn column_builder_cidr(_lua: &Lua, name: String) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(name, DataType::Cidr))
}

pub fn column_builder_enum_type(
    _lua: &Lua,
    (name, enum_name): (String, String),
) -> LuaResult<LuaColumnBuilder> {
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Enum {
            name: enum_name,
            schema: None,
        },
    ))
}

pub fn column_builder_array(
    _lua: &Lua,
    (name, element_type): (String, String),
) -> LuaResult<LuaColumnBuilder> {
    let inner_type = parse_data_type(&element_type);
    Ok(LuaColumnBuilder::new(
        name,
        DataType::Array {
            element_type: Box::new(inner_type),
        },
    ))
}
