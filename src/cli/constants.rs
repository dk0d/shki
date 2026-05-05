use crate::lua::api::{
    LuaMethodDoc as LuaMethod, LuaParamDoc as LuaParam, LuaTypeDoc as LuaTypeDef,
};
use crate::lua::{
    COLUMN_BUILDER_LUA_MODULE, COLUMN_BUILDER_LUA_TYPE, ENUM_BUILDER_LUA_MODULE,
    ENUM_BUILDER_LUA_TYPE, INDEX_BUILDER_LUA_MODULE, INDEX_BUILDER_LUA_TYPE,
    INDEX_COLUMN_LUA_MODULE, INDEX_COLUMN_LUA_TYPE, INDEX_METHOD_LUA_TABLE, MYSQL_LUA_MODULE,
    PG_LUA_MODULE, REFERENCE_ACTION_LUA_TABLE, SCHEMA_LUA_TYPE, SEQUENCE_BUILDER_LUA_MODULE,
    SEQUENCE_BUILDER_LUA_TYPE, SQLITE_LUA_MODULE, TABLE_BUILDER_LUA_MODULE, TABLE_BUILDER_LUA_TYPE,
    VIEW_BUILDER_LUA_MODULE, VIEW_BUILDER_LUA_TYPE,
};
use std::fmt::Write;

const LUA_TYPES: &[LuaTypeDef] = &[
    SCHEMA_LUA_TYPE,
    PG_LUA_MODULE,
    MYSQL_LUA_MODULE,
    SQLITE_LUA_MODULE,
    ENUM_BUILDER_LUA_MODULE,
    ENUM_BUILDER_LUA_TYPE,
    TABLE_BUILDER_LUA_MODULE,
    TABLE_BUILDER_LUA_TYPE,
    COLUMN_BUILDER_LUA_MODULE,
    COLUMN_BUILDER_LUA_TYPE,
    INDEX_BUILDER_LUA_MODULE,
    INDEX_BUILDER_LUA_TYPE,
    SEQUENCE_BUILDER_LUA_MODULE,
    SEQUENCE_BUILDER_LUA_TYPE,
    VIEW_BUILDER_LUA_MODULE,
    VIEW_BUILDER_LUA_TYPE,
    INDEX_COLUMN_LUA_MODULE,
    INDEX_COLUMN_LUA_TYPE,
    REFERENCE_ACTION_LUA_TABLE,
    INDEX_METHOD_LUA_TABLE,
];

pub const SELENE_CONFIG: &str = r#"# Selene linter configuration for shki
# https://kampfkarren.github.io/selene/

std = "shki"

[lints]
# Customize lint levels as needed
# empty_if = "warn"
# unused_variable = "warn"
"#;

fn lua_global_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for ty in LUA_TYPES.iter().filter(|ty| ty.global) {
        if !names.contains(&ty.name) {
            names.push(ty.name);
        }
    }
    names
}

fn unique_type_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for ty in LUA_TYPES {
        if !names.contains(&ty.name) {
            names.push(ty.name);
        }
    }
    names
}

fn render_luacats_method(out: &mut String, type_name: &str, method: &LuaMethod) {
    writeln!(out, "--- {}", method.doc).unwrap();
    for param in method.params {
        let optional = if param.optional { "?" } else { "" };
        writeln!(
            out,
            "---@param {}{} {} {}",
            param.name, optional, param.luacats_type, param.doc
        )
        .unwrap();
    }
    if method.is_static {
        writeln!(out, "---@return {}", method.returns).unwrap();
    } else {
        writeln!(out, "---@return {} self", method.returns).unwrap();
    }
    let joined = method
        .params
        .iter()
        .map(|param| param.name)
        .collect::<Vec<_>>()
        .join(", ");
    if method.is_static {
        writeln!(
            out,
            "function {}.{}({}) end",
            type_name, method.name, joined
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "function {}:{}({}) end",
            type_name, method.name, joined
        )
        .unwrap();
    }
    out.push('\n');
}

fn render_luacats_type(out: &mut String, name: &'static str) {
    let variants = LUA_TYPES
        .iter()
        .filter(|ty| ty.name == name)
        .collect::<Vec<_>>();
    let ty = variants[0];
    writeln!(
        out,
        "--------------------------------------------------------------------------------"
    )
    .unwrap();
    writeln!(out, "-- {}", ty.name).unwrap();
    writeln!(
        out,
        "--------------------------------------------------------------------------------\n"
    )
    .unwrap();
    writeln!(out, "--- {}", ty.doc).unwrap();
    writeln!(out, "---@class {}", ty.name).unwrap();
    let mut seen_fields = Vec::new();
    for field in variants.iter().flat_map(|ty| ty.fields.iter()) {
        if seen_fields.contains(&field.name) {
            continue;
        }
        seen_fields.push(field.name);
        writeln!(
            out,
            "---@field {} {} {}",
            field.name, field.luacats_type, field.doc
        )
        .unwrap();
    }
    let is_global = variants.iter().any(|ty| ty.global);
    let value_only = variants.iter().all(|ty| ty.methods.is_empty())
        && variants
            .iter()
            .flat_map(|ty| ty.fields.iter())
            .any(|field| field.value.is_some());
    if is_global {
        if value_only {
            writeln!(out, "{} = {{", ty.name).unwrap();
            for field in variants.iter().flat_map(|ty| ty.fields.iter()) {
                if let Some(value) = field.value {
                    writeln!(out, "    {} = \"{}\",", field.name, value).unwrap();
                }
            }
            writeln!(out, "}}\n").unwrap();
        } else {
            writeln!(out, "{} = {{}}\n", ty.name).unwrap();
        }
    } else {
        writeln!(out, "local {} = {{}}\n", ty.name).unwrap();
    }
    let mut seen_methods = Vec::new();
    for method in variants.iter().flat_map(|ty| ty.methods.iter()) {
        if seen_methods.contains(&method.name) {
            continue;
        }
        seen_methods.push(method.name);
        render_luacats_method(out, ty.name, method);
    }
}

pub fn luacats_shki_types() -> String {
    let mut out = String::from(
        "---@meta shki\n--- Shki Lua API Type Definitions\n--- Generated from the Rust binding metadata.\n--- This file provides type information for the Lua Language Server.\n--- It should NOT be executed - it is only for IDE support.\n\n",
    );

    for name in unique_type_names() {
        render_luacats_type(&mut out, name);
    }

    out
}

pub fn luarc_config() -> String {
    let globals = lua_global_names()
        .into_iter()
        .map(|name| format!("      \"{}\"", name))
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "{{\n  \"$schema\": \"https://raw.githubusercontent.com/LuaLS/vscode-lua/master/setting/schema.json\",\n  \"runtime\": {{\n    \"version\": \"Lua 5.4\"\n  }},\n  \"workspace\": {{\n    \"library\": [\n      \".luacats\"\n    ],\n    \"checkThirdParty\": false\n  }},\n  \"diagnostics\": {{\n    \"globals\": [\n{}\n    ]\n  }}\n}}\n",
        globals
    )
}

fn render_selene_args(out: &mut String, params: &[LuaParam]) {
    if params.is_empty() {
        out.push_str("        args: []\n");
        return;
    }

    out.push_str("        args:\n");
    for param in params {
        writeln!(out, "          - type: {}", param.selene_type).unwrap();
        if param.optional {
            out.push_str("            required: false\n");
        }
    }
}

pub fn selene_shki_std() -> String {
    let mut out = String::from(
        "---\n# Shki standard library definition for Selene\n# Generated from the Rust binding metadata.\n\nbase: lua54\n\nglobals:\n",
    );

    for ty in LUA_TYPES.iter().filter(|ty| ty.global) {
        writeln!(out, "  {}:", ty.name).unwrap();
        out.push_str("    property: read-only\n");
        out.push_str("    struct:\n");

        if ty.fields.iter().any(|field| field.value.is_some()) && ty.methods.is_empty() {
            for field in ty.fields {
                writeln!(out, "      {}:", field.name).unwrap();
                out.push_str("        property: read-only\n");
            }
            continue;
        }

        for method in ty.methods.iter().filter(|method| method.is_static) {
            writeln!(out, "      {}:", method.name).unwrap();
            render_selene_args(&mut out, method.params);
            out.push_str("        method: false\n");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_luacats_includes_all_lua_globals_and_interfaces() {
        let luacats = luacats_shki_types();

        for global in lua_global_names() {
            assert!(
                luacats.contains(global),
                "missing {global} in LuaCATS output"
            );
        }

        assert!(luacats.contains("function Schema:enum(enum_type) end"));
        assert!(luacats.contains("function Schema:sequence(sequence) end"));
        assert!(luacats.contains("function Schema:view(view) end"));
        assert!(luacats.contains("function ColumnBuilder.enum(name, enum_name) end"));
        assert!(luacats.contains("function IndexBuilder:index_column(col) end"));
        assert!(luacats.contains("function SequenceBuilder:cycle() end"));
        assert!(luacats.contains("function ViewBuilder:materialized() end"));
        assert!(luacats.contains("function IndexColumn.expression(expr) end"));
    }

    #[test]
    fn generated_luarc_includes_registered_globals() {
        let luarc = luarc_config();

        for global in [
            "SequenceBuilder",
            "ViewBuilder",
            "IndexColumn",
            "ReferenceAction",
            "IndexMethod",
        ] {
            assert!(luarc.contains(&format!("\"{}\"", global)));
        }
    }

    #[test]
    fn generated_selene_std_includes_runtime_globals() {
        let selene = selene_shki_std();

        for global in [
            "EnumBuilder:",
            "ColumnBuilder:",
            "SequenceBuilder:",
            "ViewBuilder:",
            "IndexColumn:",
        ] {
            assert!(selene.contains(global));
        }

        assert!(selene.contains("      enum:"));
        assert!(selene.contains("      new:"));
        assert!(selene.contains("      schema:"));
    }
}
