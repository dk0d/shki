/// LuaCATS type definitions for shki
pub const LUACATS_SHKI_TYPES: &str = "pending";

/// Lua Language Server configuration
pub const LUARC_CONFIG: &str = r#"{
  "$schema": "https://raw.githubusercontent.com/LuaLS/vscode-lua/master/setting/schema.json",
  "runtime": {
    "version": "Lua 5.4"
  },
  "workspace": {
    "library": [
      ".luacats"
    ],
    "checkThirdParty": false
  },
  "diagnostics": {
    "globals": [
      "pg",
      "mysql",
      "sqlite",
      "EnumBuilder",
      "TableBuilder",
      "ColumnBuilder",
      "IndexBuilder",
      "ReferentialAction",
      "IndexMethod"
    ]
  }
}
"#;

/// Selene linter configuration
pub const SELENE_CONFIG: &str = r#"# Selene linter configuration for shki
# https://kampfkarren.github.io/selene/

std = "lua54+shki"

[lints]
# Customize lint levels as needed
# empty_if = "warn"
# unused_variable = "warn"
"#;

pub const SELENE_SHKI_STD: &str = "";
