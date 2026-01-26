//! Lua bindings for schema types
//!
//! This module provides UserData implementations for schema types to be used from Lua.

use mlua::{FromLua, Lua, MetaMethod, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{
    Column, EnumBuilder, EnumType, Index, IndexMethod, Schema, Table,
    types::{DataType, DefaultValue, ReferenceAction},
};

// ============================================================================
// LuaSchema - Wrapper for Schema
// ============================================================================

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

        // enum_type(enum_builder) -> self
        methods.add_method("enum_type", |_, this, enum_type: LuaEnumBuilder| {
            let enum_type = enum_type.build();
            this.inner.borrow_mut().enum_type(enum_type);
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

// ============================================================================
// LuaEnumBuilder - Wrapper for EnumBuilder
// ============================================================================

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

// ============================================================================
// LuaTableBuilder - Wrapper for TableBuilder
// ============================================================================

/// Lua wrapper for TableBuilder
#[derive(Clone)]
pub struct LuaTableBuilder {
    inner: Rc<RefCell<Table>>,
}

impl LuaTableBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Table::new(name))),
        }
    }

    pub fn build(self) -> Table {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner())
            .unwrap_or_else(|rc| rc.borrow().clone())
    }
}

impl UserData for LuaTableBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // schema(name) -> self
        methods.add_method("schema", |_, this, schema: String| {
            this.inner.borrow_mut().schema = Some(schema);
            Ok(this.clone())
        });

        // description(desc) -> self  (alias for comment)
        methods.add_method("description", |_, this, desc: String| {
            this.inner.borrow_mut().comment = Some(desc);
            Ok(this.clone())
        });

        // comment(text) -> self
        methods.add_method("comment", |_, this, comment: String| {
            this.inner.borrow_mut().comment = Some(comment);
            Ok(this.clone())
        });

        // column(column_builder) -> self
        methods.add_method("column", |_, this, column: LuaColumnBuilder| {
            let col = column.build();
            this.inner.borrow_mut().column(col);
            Ok(this.clone())
        });

        // primary_key(columns) -> self
        methods.add_method("primary_key", |_, this, columns: Vec<String>| {
            use crate::schema::{Constraint, PrimaryKeyConstraint};
            this.inner
                .borrow_mut()
                .constraint(Constraint::PrimaryKey(PrimaryKeyConstraint::new(columns)));
            Ok(this.clone())
        });

        // unique_constraint(columns) -> self
        methods.add_method("unique_constraint", |_, this, columns: Vec<String>| {
            use crate::schema::{Constraint, UniqueConstraint};
            this.inner
                .borrow_mut()
                .constraint(Constraint::Unique(UniqueConstraint::new(columns)));
            Ok(this.clone())
        });

        // foreign_key(columns, ref_table, ref_columns) -> self
        methods.add_method(
            "foreign_key",
            |_, this, (columns, ref_table, ref_columns): (Vec<String>, String, Vec<String>)| {
                use crate::schema::{Constraint, ForeignKeyConstraint};
                this.inner.borrow_mut().constraint(Constraint::ForeignKey(
                    ForeignKeyConstraint::new(columns, ref_table, ref_columns),
                ));
                Ok(this.clone())
            },
        );

        // foreign_key_with_actions(columns, ref_table, ref_columns, on_delete, on_update) -> self
        methods.add_method(
            "foreign_key_with_actions",
            |_,
             this,
             (columns, ref_table, ref_columns, on_delete, on_update): (
                Vec<String>,
                String,
                Vec<String>,
                String,
                String,
            )| {
                use crate::schema::{Constraint, ForeignKeyConstraint};
                let mut fk = ForeignKeyConstraint::new(columns, ref_table, ref_columns);
                fk.on_delete = parse_referential_action(&on_delete);
                fk.on_update = parse_referential_action(&on_update);
                this.inner
                    .borrow_mut()
                    .constraint(Constraint::ForeignKey(fk));
                Ok(this.clone())
            },
        );

        // check(expression) -> self
        methods.add_method("check", |_, this, expression: String| {
            use crate::schema::{CheckConstraint, Constraint};
            this.inner
                .borrow_mut()
                .constraint(Constraint::Check(CheckConstraint::new(expression)));
            Ok(this.clone())
        });

        // index(name, columns) -> self
        methods.add_method(
            "index",
            |_, this, (name, columns): (String, Vec<String>)| {
                let index = Index::new(name, columns);
                this.inner.borrow_mut().index(index);
                Ok(this.clone())
            },
        );

        // unique_index(name, columns) -> self
        methods.add_method(
            "unique_index",
            |_, this, (name, columns): (String, Vec<String>)| {
                let index = Index::new(name, columns).unique();
                this.inner.borrow_mut().index(index);
                Ok(this.clone())
            },
        );

        // index_with(index_builder) -> self
        methods.add_method("index_with", |_, this, index: LuaIndexBuilder| {
            let idx = index.build();
            this.inner.borrow_mut().index(idx);
            Ok(this.clone())
        });
    }
}

impl FromLua for LuaTableBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaTableBuilder".to_string(),
                message: Some("expected TableBuilder".to_string()),
            }),
        }
    }
}

// ============================================================================
// LuaColumnBuilder - Wrapper for ColumnBuilder
// ============================================================================

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
// LuaIndexBuilder - Wrapper for IndexBuilder
// ============================================================================

/// Lua wrapper for IndexBuilder
#[derive(Clone)]
pub struct LuaIndexBuilder {
    inner: Rc<RefCell<Index>>,
}

impl LuaIndexBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Index::new(name, Vec::<String>::new()))),
        }
    }

    pub fn build(self) -> Index {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner())
            .unwrap_or_else(|rc| rc.borrow().clone())
    }
}

impl UserData for LuaIndexBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // column(name) -> self
        methods.add_method("column", |_, this, name: String| {
            use crate::schema::index::IndexColumn;
            this.inner
                .borrow_mut()
                .columns
                .push(IndexColumn::column(name));
            Ok(this.clone())
        });

        // columns(names) -> self
        methods.add_method("columns", |_, this, names: Vec<String>| {
            use crate::schema::index::IndexColumn;
            for name in names {
                this.inner
                    .borrow_mut()
                    .columns
                    .push(IndexColumn::column(name));
            }
            Ok(this.clone())
        });

        // expression(expr) -> self
        methods.add_method("expression", |_, this, expr: String| {
            use crate::schema::index::IndexColumn;
            this.inner
                .borrow_mut()
                .columns
                .push(IndexColumn::expression(expr));
            Ok(this.clone())
        });

        // unique() -> self
        methods.add_method("unique", |_, this, ()| {
            this.inner.borrow_mut().unique = true;
            Ok(this.clone())
        });

        // using(method) -> self
        methods.add_method("using", |_, this, method: String| {
            this.inner.borrow_mut().method = parse_index_method(&method);
            Ok(this.clone())
        });

        // where_clause(clause) -> self
        methods.add_method("where_clause", |_, this, clause: String| {
            this.inner.borrow_mut().where_clause = Some(clause);
            Ok(this.clone())
        });

        // include(columns) -> self
        methods.add_method("include", |_, this, columns: Vec<String>| {
            this.inner.borrow_mut().include = columns;
            Ok(this.clone())
        });

        // concurrently() -> self
        methods.add_method("concurrently", |_, this, ()| {
            this.inner.borrow_mut().concurrently = true;
            Ok(this.clone())
        });
    }
}

impl FromLua for LuaIndexBuilder {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|s| s.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaIndexBuilder".to_string(),
                message: Some("expected IndexBuilder".to_string()),
            }),
        }
    }
}

// ============================================================================
// Helper functions to parse enum values from strings
// ============================================================================

fn parse_referential_action(s: &str) -> ReferenceAction {
    match s.to_lowercase().as_str() {
        "cascade" => ReferenceAction::Cascade,
        "restrict" => ReferenceAction::Restrict,
        "set_null" | "setnull" => ReferenceAction::SetNull,
        "set_default" | "setdefault" => ReferenceAction::SetDefault,
        _ => ReferenceAction::NoAction,
    }
}

fn parse_index_method(s: &str) -> IndexMethod {
    match s.to_lowercase().as_str() {
        "hash" => IndexMethod::Hash,
        "gist" => IndexMethod::Gist,
        "spgist" => IndexMethod::SpGist,
        "gin" => IndexMethod::Gin,
        "brin" => IndexMethod::Brin,
        _ => IndexMethod::BTree,
    }
}

// ============================================================================
// Lua function implementations
// ============================================================================

/// pg.schema(name) -> LuaSchema
pub fn pg_schema(_lua: &Lua, name: String) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::new(Schema::postgres(name)))
}

/// mysql.schema(name) -> LuaSchema
pub fn mysql_schema(_lua: &Lua, name: String) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::new(Schema::mysql(name)))
}

/// sqlite.schema() -> LuaSchema
pub fn sqlite_schema(_lua: &Lua, _: ()) -> LuaResult<LuaSchema> {
    Ok(LuaSchema::new(Schema::sqlite()))
}

/// EnumBuilder.new(name) -> LuaEnumBuilder
pub fn enum_builder_new(_lua: &Lua, name: String) -> LuaResult<LuaEnumBuilder> {
    Ok(LuaEnumBuilder::new(name))
}

/// TableBuilder.new(name) -> LuaTableBuilder
pub fn table_builder_new(_lua: &Lua, name: String) -> LuaResult<LuaTableBuilder> {
    Ok(LuaTableBuilder::new(name))
}

/// IndexBuilder.new(name) -> LuaIndexBuilder
pub fn index_builder_new(_lua: &Lua, name: String) -> LuaResult<LuaIndexBuilder> {
    Ok(LuaIndexBuilder::new(name))
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

/// Parse a string into a DataType
fn parse_data_type(s: &str) -> DataType {
    match s.to_lowercase().as_str() {
        "serial" => DataType::Serial,
        "bigserial" => DataType::BigSerial,
        "smallserial" => DataType::SmallSerial,
        "integer" | "int" | "int4" => DataType::Integer,
        "bigint" | "int8" => DataType::BigInt,
        "smallint" | "int2" => DataType::SmallInt,
        "text" => DataType::Text,
        "varchar" => DataType::VarChar { length: None },
        "char" => DataType::Char { length: None },
        "boolean" | "bool" => DataType::Boolean,
        "timestamp" => DataType::Timestamp {
            precision: None,
            with_timezone: false,
        },
        "timestamptz" => DataType::Timestamp {
            precision: None,
            with_timezone: true,
        },
        "date" => DataType::Date,
        "time" => DataType::Time {
            precision: None,
            with_timezone: false,
        },
        "uuid" => DataType::Uuid,
        "json" => DataType::Json,
        "jsonb" => DataType::JsonB,
        "real" | "float4" => DataType::Real,
        "double precision" | "float8" => DataType::DoublePrecision,
        "bytea" => DataType::ByteA,
        "inet" => DataType::Inet,
        "cidr" => DataType::Cidr,
        _ => DataType::Text, // Default fallback
    }
}
