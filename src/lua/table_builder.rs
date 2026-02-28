//! Lua wrapper for TableBuilder

use mlua::{FromLua, Lua, Result as LuaResult, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::schema::{Table, TableBuilder};

use super::helpers::parse_referential_action;
use super::{LuaColumnBuilder, LuaIndexBuilder};

/// Lua wrapper for TableBuilder
#[derive(Clone)]
pub struct LuaTableBuilder {
    inner: Rc<RefCell<TableBuilder>>,
}

impl LuaTableBuilder {
    pub fn new(name: String) -> Self {
        Self {
            inner: Rc::new(RefCell::new(TableBuilder::new(name))),
        }
    }

    fn transform<F>(&self, f: F)
    where
        F: FnOnce(TableBuilder) -> TableBuilder,
    {
        let updated = {
            let current = self.inner.borrow().clone();
            f(current)
        };
        *self.inner.borrow_mut() = updated;
    }

    pub fn build(self) -> Table {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner().build())
            .unwrap_or_else(|rc| rc.borrow().clone().build())
    }
}

impl UserData for LuaTableBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // schema(name) -> self
        methods.add_method("schema", |_, this, schema: String| {
            this.transform(|builder| builder.schema(schema));
            Ok(this.clone())
        });

        // description(desc) -> self  (alias for comment)
        methods.add_method("description", |_, this, desc: String| {
            this.transform(|builder| builder.description(desc));
            Ok(this.clone())
        });

        // comment(text) -> self
        methods.add_method("comment", |_, this, comment: String| {
            this.transform(|builder| builder.comment(comment));
            Ok(this.clone())
        });

        // column(column_builder) -> self
        methods.add_method("column", |_, this, column: LuaColumnBuilder| {
            let col = column.build();
            this.transform(|builder| builder.column(col));
            Ok(this.clone())
        });

        // primary_key(columns) -> self
        methods.add_method("primary_key", |_, this, columns: Vec<String>| {
            this.transform(|builder| builder.primary_key(columns));
            Ok(this.clone())
        });

        // unique_constraint(columns) -> self
        methods.add_method("unique_constraint", |_, this, columns: Vec<String>| {
            this.transform(|builder| builder.unique_constraint(columns));
            Ok(this.clone())
        });

        // foreign_key(columns, ref_table, ref_columns) -> self
        methods.add_method(
            "foreign_key",
            |_, this, (columns, ref_table, ref_columns): (Vec<String>, String, Vec<String>)| {
                this.transform(|builder| builder.foreign_key(columns, ref_table, ref_columns));
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
                let on_delete = parse_referential_action(&on_delete);
                let on_update = parse_referential_action(&on_update);
                this.transform(|builder| {
                    builder.foreign_key_with_actions(
                        columns,
                        ref_table,
                        ref_columns,
                        on_delete,
                        on_update,
                    )
                });
                Ok(this.clone())
            },
        );

        // check(expression) -> self
        methods.add_method("check", |_, this, expression: String| {
            this.transform(|builder| builder.check(expression));
            Ok(this.clone())
        });

        // index(name, columns) -> self
        methods.add_method(
            "index",
            |_, this, (name, columns): (String, Vec<String>)| {
                this.transform(|builder| builder.index(name, columns));
                Ok(this.clone())
            },
        );

        // unique_index(name, columns) -> self
        methods.add_method(
            "unique_index",
            |_, this, (name, columns): (String, Vec<String>)| {
                this.transform(|builder| builder.unique_index(name, columns));
                Ok(this.clone())
            },
        );

        // index_with(index_builder) -> self
        methods.add_method("index_with", |_, this, index: LuaIndexBuilder| {
            let idx = index.build();
            this.transform(|builder| builder.index_with(idx));
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

/// TableBuilder.new(name) -> LuaTableBuilder
pub fn table_builder_new(_: &Lua, name: String) -> LuaResult<LuaTableBuilder> {
    Ok(LuaTableBuilder::new(name))
}
