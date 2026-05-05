use mlua::{Lua, Table};

use crate::{Result, ShkiError};

#[derive(Clone, Copy)]
pub(crate) struct LuaParamDoc {
    pub(crate) name: &'static str,
    pub(crate) luacats_type: &'static str,
    pub(crate) selene_type: &'static str,
    pub(crate) doc: &'static str,
    pub(crate) optional: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct LuaMethodDoc {
    pub(crate) name: &'static str,
    pub(crate) doc: &'static str,
    pub(crate) params: &'static [LuaParamDoc],
    pub(crate) returns: &'static str,
    pub(crate) is_static: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct LuaFieldDoc {
    pub(crate) name: &'static str,
    pub(crate) luacats_type: &'static str,
    pub(crate) doc: &'static str,
    pub(crate) value: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(crate) struct LuaTypeDoc {
    pub(crate) name: &'static str,
    pub(crate) doc: &'static str,
    pub(crate) fields: &'static [LuaFieldDoc],
    pub(crate) methods: &'static [LuaMethodDoc],
    pub(crate) global: bool,
}

pub(crate) fn register_global_table(
    lua: &Lua,
    globals: &Table,
    name: &'static str,
) -> Result<Table> {
    let table = lua
        .create_table()
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    globals
        .set(name, table.clone())
        .map_err(|e| ShkiError::lua(e.to_string()))?;
    Ok(table)
}

/// Defines a shki Lua global table/module and emits the paired runtime registration
/// function and API metadata entry used by generated Lua support files.
///
/// Use this for globals that appear directly in Lua, such as dialect modules like
/// `pg` / `mysql` / `sqlite`, and constructor tables like `ColumnBuilder`.
#[macro_export]
macro_rules! lua_global_module {
    (
        metadata: $meta:ident,
        register: $register:ident,
        name: $name:literal,
        doc: $doc:expr,
        functions: [
            $(
                fn $fn_name:literal ( $( $param_name:ident : $param_ty:ty => ($lua_ty:expr, $selene_ty:expr, $param_doc:expr $(, $optional:ident )? ) ),* $(,)? ) -> $ret:expr => $handler:path ;
            )*
        ],
    ) => {
        pub(crate) const $meta: $crate::lua::api::LuaTypeDoc = $crate::lua::api::LuaTypeDoc {
            name: $name,
            doc: $doc,
            fields: &[],
            methods: &[
                $(
                    $crate::lua::api::LuaMethodDoc {
                        name: $fn_name,
                        doc: concat!("Lua global function `", $name, ".", $fn_name, "`."),
                        params: &[
                            $(
                                $crate::lua::api::LuaParamDoc {
                                    name: stringify!($param_name),
                                    luacats_type: $lua_ty,
                                    selene_type: $selene_ty,
                                    doc: $param_doc,
                                    optional: $crate::lua_global_module!(@optional $( $optional )?),
                                }
                            ),*
                        ],
                        returns: $ret,
                        is_static: true,
                    }
                ),*
            ],
            global: true,
        };

        pub(crate) fn $register(lua: &mlua::Lua, globals: &mlua::Table) -> $crate::Result<()> {
            let table = $crate::lua::api::register_global_table(lua, globals, $name)?;
            $(
                table
                    .set(
                        $fn_name,
                        lua.create_function($handler)
                            .map_err(|e| $crate::ShkiError::lua(e.to_string()))?,
                    )
                    .map_err(|e| $crate::ShkiError::lua(e.to_string()))?;
            )*
            Ok(())
        }
    };

    (@optional optional) => { true };
    (@optional) => { false };
}

/// Defines the instance-method surface for a shki Lua userdata wrapper and emits
/// both the `mlua` method registration helper and the metadata used for generated
/// Lua typings and linter configuration.
///
/// Use this for chainable builder APIs like `Schema:table(...)` or
/// `ColumnBuilder:not_null()` so runtime behavior and generated Lua definitions stay aligned.
#[macro_export]
macro_rules! lua_builder_def {
    (
        target: $target:ty,
        metadata: $meta:ident,
        register: $register:ident,
        type_name: $type_name:literal,
        doc: $doc:expr,
        fields: [ $( field $field_name:ident : $field_ty:expr => $field_doc:expr; )* ],
        methods: [
            $(
                method $method_name:literal ( $( $param_name:ident : $param_ty:ty => ($lua_ty:expr, $selene_ty:expr, $param_doc:expr $(, $optional:ident )? ) ),* $(,)? ) -> $ret:expr => |$this:ident $(, $arg:ident )*| $body:block ;
            )*
        ],
    ) => {
        pub(crate) const $meta: $crate::lua::api::LuaTypeDoc = $crate::lua::api::LuaTypeDoc {
            name: $type_name,
            doc: $doc,
            fields: &[
                $(
                    $crate::lua::api::LuaFieldDoc {
                        name: stringify!($field_name),
                        luacats_type: $field_ty,
                        doc: $field_doc,
                        value: None,
                    }
                ),*
            ],
            methods: &[
                $(
                    $crate::lua::api::LuaMethodDoc {
                        name: $method_name,
                        doc: concat!("Lua method `", $type_name, ":", $method_name, "`."),
                        params: &[
                            $(
                                $crate::lua::api::LuaParamDoc {
                                    name: stringify!($param_name),
                                    luacats_type: $lua_ty,
                                    selene_type: $selene_ty,
                                    doc: $param_doc,
                                    optional: $crate::lua_builder_def!(@optional $( $optional )?),
                                }
                            ),*
                        ],
                        returns: $ret,
                        is_static: false,
                    }
                ),*
            ],
            global: false,
        };

        pub(crate) fn $register<M: mlua::UserDataMethods<$target>>(methods: &mut M) {
            $(
                methods.add_method(
                    $method_name,
                    |_, this, ( $( $param_name, )* ): ( $( $param_ty, )* )| {
                        let $this = this;
                        $( let $arg = $param_name; )*
                        $body
                    },
                );
            )*
        }
    };

    (@optional optional) => { true };
    (@optional) => { false };
}

/// Defines a shki Lua constant table and emits both runtime registration code and
/// metadata for generated Lua support files.
///
/// Use this for enum-like globals such as `ReferenceAction` and `IndexMethod`, where
/// Lua should see a stable table of named string values that matches the documented API.
#[macro_export]
macro_rules! lua_value_table {
    (
        metadata: $meta:ident,
        register: $register:ident,
        name: $name:literal,
        doc: $doc:expr,
        values: [ $( $field_name:ident = $value:literal => $field_doc:expr; )* ],
    ) => {
        pub(crate) const $meta: $crate::lua::api::LuaTypeDoc = $crate::lua::api::LuaTypeDoc {
            name: $name,
            doc: $doc,
            fields: &[
                $(
                    $crate::lua::api::LuaFieldDoc {
                        name: stringify!($field_name),
                        luacats_type: "string",
                        doc: $field_doc,
                        value: Some($value),
                    }
                ),*
            ],
            methods: &[],
            global: true,
        };

        pub(crate) fn $register(lua: &mlua::Lua, globals: &mlua::Table) -> $crate::Result<()> {
            let table = lua
                .create_table()
                .map_err(|e| $crate::ShkiError::lua(e.to_string()))?;
            $(
                table
                    .set(stringify!($field_name), $value)
                    .map_err(|e| $crate::ShkiError::lua(e.to_string()))?;
            )*
            globals
                .set($name, table)
                .map_err(|e| $crate::ShkiError::lua(e.to_string()))?;
            Ok(())
        }
    };
}
