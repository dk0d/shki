/// LuaCATS type definitions for shki
pub const LUACATS_SHKI_TYPES: &str = r#"---@meta shki
--- Shki Lua API Type Definitions
--- This file provides type information for the Lua Language Server.
--- It should NOT be executed - it's only for IDE support.

--------------------------------------------------------------------------------
-- Enums / Constants
--------------------------------------------------------------------------------

---@class ReferenceAction
---@field NoAction string
---@field Restrict string
---@field Cascade string
---@field SetNull string
---@field SetDefault string
ReferenceAction = {
    NoAction = "no_action",
    Restrict = "restrict",
    Cascade = "cascade",
    SetNull = "set_null",
    SetDefault = "set_default",
}

---@class IndexMethod
---@field BTree string
---@field Hash string
---@field Gist string
---@field SpGist string
---@field Gin string
---@field Brin string
IndexMethod = {
    BTree = "btree",
    Hash = "hash",
    Gist = "gist",
    SpGist = "spgist",
    Gin = "gin",
    Brin = "brin",
}

--------------------------------------------------------------------------------
-- Schema
--------------------------------------------------------------------------------

---@class Schema
---@field name string The schema name (e.g., "public")
---@field dialect string The database dialect ("Postgres", "Mysql", "Sqlite")
local Schema = {}

--- Add a table to the schema
---@param table TableBuilder The table builder
---@return Schema self
function Schema:table(table) end

--- Add an enum type to the schema
---@param enum EnumBuilder The enum builder
---@return Schema self
function Schema:enum_type(enum) end

--- Add a PostgreSQL extension
---@param name string Extension name (e.g., "uuid-ossp")
---@return Schema self
function Schema:extension(name) end

--------------------------------------------------------------------------------
-- Dialect Modules
--------------------------------------------------------------------------------

---@class pg
pg = {}

--- Create a new PostgreSQL schema
---@param name string Schema name (e.g., "public")
---@return Schema
function pg.schema(name) end

---@class mysql
mysql = {}

--- Create a new MySQL schema/database
---@param name string Database name
---@return Schema
function mysql.schema(name) end

---@class sqlite
sqlite = {}

--- Create a new SQLite schema
---@return Schema
function sqlite.schema() end

--------------------------------------------------------------------------------
-- EnumBuilder
--------------------------------------------------------------------------------

---@class EnumBuilder
EnumBuilder = {}

--- Create a new enum builder
---@param name string The enum type name
---@return EnumBuilder
function EnumBuilder.new(name) end

--- Add a single value to the enum
---@param value string The enum value
---@return EnumBuilder self
function EnumBuilder:value(value) end

--- Add multiple values to the enum
---@param values string[] Array of enum values
---@return EnumBuilder self
function EnumBuilder:values(values) end

--- Set the description/comment for the enum
---@param desc string Description text (supports markdown)
---@return EnumBuilder self
function EnumBuilder:description(desc) end

--------------------------------------------------------------------------------
-- TableBuilder
--------------------------------------------------------------------------------

---@class TableBuilder
TableBuilder = {}

--- Create a new table builder
---@param name string The table name
---@return TableBuilder
function TableBuilder.new(name) end

--- Set the schema name for the table
---@param name string Schema name
---@return TableBuilder self
function TableBuilder:schema(name) end

--- Set the description/comment for the table
---@param desc string Description text (supports markdown)
---@return TableBuilder self
function TableBuilder:description(desc) end

--- Set a comment for the table (alias for description)
---@param text string Comment text
---@return TableBuilder self
function TableBuilder:comment(text) end

--- Add a column to the table
---@param column ColumnBuilder The column builder
---@return TableBuilder self
function TableBuilder:column(column) end

--- Add a composite primary key constraint
---@param columns string[] Column names
---@return TableBuilder self
function TableBuilder:primary_key(columns) end

--- Add a unique constraint on columns
---@param columns string[] Column names
---@return TableBuilder self
function TableBuilder:unique_constraint(columns) end

--- Add a foreign key constraint
---@param columns string[] Local column names
---@param ref_table string Referenced table name
---@param ref_columns string[] Referenced column names
---@return TableBuilder self
function TableBuilder:foreign_key(columns, ref_table, ref_columns) end

--- Add a foreign key constraint with referential actions
---@param columns string[] Local column names
---@param ref_table string Referenced table name
---@param ref_columns string[] Referenced column names
---@param on_delete string ON DELETE action (use ReferenceAction.*)
---@param on_update string ON UPDATE action (use ReferenceAction.*)
---@return TableBuilder self
function TableBuilder:foreign_key_with_actions(columns, ref_table, ref_columns, on_delete, on_update) end

--- Add a check constraint
---@param expression string SQL check expression
---@return TableBuilder self
function TableBuilder:check(expression) end

--- Add an index on columns
---@param name string Index name
---@param columns string[] Column names
---@return TableBuilder self
function TableBuilder:index(name, columns) end

--- Add a unique index on columns
---@param name string Index name
---@param columns string[] Column names
---@return TableBuilder self
function TableBuilder:unique_index(name, columns) end

--- Add an index using IndexBuilder
---@param index IndexBuilder The index builder
---@return TableBuilder self
function TableBuilder:index_with(index) end

--------------------------------------------------------------------------------
-- ColumnBuilder
--------------------------------------------------------------------------------

---@class ColumnBuilder
ColumnBuilder = {}

--- Create a column with a custom type
---@param name string Column name
---@param type_name string SQL type name
---@return ColumnBuilder
function ColumnBuilder.new(name, type_name) end

--- Create a SERIAL column (auto-incrementing integer)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.serial(name) end

--- Create a BIGSERIAL column (auto-incrementing bigint)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.bigserial(name) end

--- Create a SMALLSERIAL column (auto-incrementing smallint)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.smallserial(name) end

--- Create an INTEGER column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.integer(name) end

--- Create a BIGINT column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.bigint(name) end

--- Create a SMALLINT column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.smallint(name) end

--- Create a TEXT column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.text(name) end

--- Create a VARCHAR column
---@param name string Column name
---@param length? integer Optional max length
---@return ColumnBuilder
function ColumnBuilder.varchar(name, length) end

--- Create a CHAR column
---@param name string Column name
---@param length? integer Optional fixed length
---@return ColumnBuilder
function ColumnBuilder.char(name, length) end

--- Create a BOOLEAN column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.boolean(name) end

--- Create a TIMESTAMP column (without timezone)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.timestamp(name) end

--- Create a TIMESTAMP WITH TIME ZONE column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.timestamptz(name) end

--- Create a DATE column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.date(name) end

--- Create a TIME column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.time(name) end

--- Create a UUID column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.uuid(name) end

--- Create a JSON column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.json(name) end

--- Create a JSONB column (PostgreSQL binary JSON)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.jsonb(name) end

--- Create a NUMERIC/DECIMAL column
---@param name string Column name
---@param precision? integer Total number of digits
---@param scale? integer Number of decimal places
---@return ColumnBuilder
function ColumnBuilder.numeric(name, precision, scale) end

--- Create a REAL column (single precision float)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.real(name) end

--- Create a DOUBLE PRECISION column
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.double_precision(name) end

--- Create a BYTEA column (binary data)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.bytea(name) end

--- Create an INET column (IP address)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.inet(name) end

--- Create a CIDR column (network address)
---@param name string Column name
---@return ColumnBuilder
function ColumnBuilder.cidr(name) end

--- Create a column with an enum type
---@param name string Column name
---@param enum_name string Name of the enum type
---@return ColumnBuilder
function ColumnBuilder.enum_type(name, enum_name) end

--- Create an array column
---@param name string Column name
---@param element_type string Element type name
---@return ColumnBuilder
function ColumnBuilder.array(name, element_type) end

-- Column modifiers

--- Mark the column as NOT NULL
---@return ColumnBuilder self
function ColumnBuilder:not_null() end

--- Mark the column as nullable (default)
---@return ColumnBuilder self
function ColumnBuilder:nullable() end

--- Mark the column as a primary key
---@return ColumnBuilder self
function ColumnBuilder:primary_key() end

--- Add a UNIQUE constraint to the column
---@return ColumnBuilder self
function ColumnBuilder:unique() end

--- Set a default value (literal)
---@param value string Default value as SQL literal
---@return ColumnBuilder self
function ColumnBuilder:default_value(value) end

--- Set default to CURRENT_TIMESTAMP/now()
---@return ColumnBuilder self
function ColumnBuilder:default_now() end

--- Set a default expression
---@param expr string SQL expression for default
---@return ColumnBuilder self
function ColumnBuilder:default_expression(expr) end

--- Set the description/comment for the column
---@param desc string Description text (supports markdown)
---@return ColumnBuilder self
function ColumnBuilder:description(desc) end

--- Set a comment for the column (alias for description)
---@param text string Comment text
---@return ColumnBuilder self
function ColumnBuilder:comment(text) end

--- Set the collation for a text column
---@param collation string Collation name
---@return ColumnBuilder self
function ColumnBuilder:collate(collation) end

--- Add a foreign key reference to another table
---@param table string Referenced table name
---@param column string Referenced column name
---@return ColumnBuilder self
function ColumnBuilder:references(table, column) end

--- Add a foreign key reference with ON DELETE action
---@param table string Referenced table name
---@param column string Referenced column name
---@param action string ON DELETE action (use ReferenceAction.*)
---@return ColumnBuilder self
function ColumnBuilder:references_on_delete(table, column, action) end

--- Make this an identity column
---@param always boolean If true, ALWAYS; if false, BY DEFAULT
---@return ColumnBuilder self
function ColumnBuilder:identity(always) end

--- Make this a generated/computed column
---@param expression string SQL expression to compute value
---@param stored boolean If true, value is stored; if false, virtual
---@return ColumnBuilder self
function ColumnBuilder:generated_as(expression, stored) end

--------------------------------------------------------------------------------
-- IndexBuilder
--------------------------------------------------------------------------------

---@class IndexBuilder
IndexBuilder = {}

--- Create a new index builder
---@param name string Index name
---@return IndexBuilder
function IndexBuilder.new(name) end

--- Add a column to the index
---@param name string Column name
---@return IndexBuilder self
function IndexBuilder:column(name) end

--- Add multiple columns to the index
---@param names string[] Column names
---@return IndexBuilder self
function IndexBuilder:columns(names) end

--- Add an expression to the index
---@param expr string SQL expression
---@return IndexBuilder self
function IndexBuilder:expression(expr) end

--- Make this a unique index
---@return IndexBuilder self
function IndexBuilder:unique() end

--- Set the index method (btree, hash, gist, etc.)
---@param method string Index method (use IndexMethod.*)
---@return IndexBuilder self
function IndexBuilder:using(method) end

--- Add a WHERE clause for a partial index
---@param clause string SQL WHERE clause (without WHERE keyword)
---@return IndexBuilder self
function IndexBuilder:where_clause(clause) end

--- Add INCLUDE columns (PostgreSQL covering index)
---@param columns string[] Column names to include
---@return IndexBuilder self
function IndexBuilder:include(columns) end

--- Create the index concurrently (non-blocking)
---@return IndexBuilder self
function IndexBuilder:concurrently() end
"#;

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
      "ReferenceAction",
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

pub const SELENE_SHKI_STD: &str = r#"---
# Shki standard library definition for Selene
# This file defines the globals injected by the shki Lua runtime

base: lua54

globals:
  # Dialect modules
  pg:
    property: read-only
    struct:
      schema:
        args:
          - type: string
        method: false

  mysql:
    property: read-only
    struct:
      schema:
        args:
          - type: string
        method: false

  sqlite:
    property: read-only
    struct:
      schema:
        args: []
        method: false

  # Builder classes
  EnumBuilder:
    property: read-only
    struct:
      new:
        args:
          - type: string
        method: false

  TableBuilder:
    property: read-only
    struct:
      new:
        args:
          - type: string
        method: false

  ColumnBuilder:
    property: read-only
    struct:
      new:
        args:
          - type: string
          - type: string
        method: false
      serial:
        args:
          - type: string
        method: false
      bigserial:
        args:
          - type: string
        method: false
      smallserial:
        args:
          - type: string
        method: false
      integer:
        args:
          - type: string
        method: false
      bigint:
        args:
          - type: string
        method: false
      smallint:
        args:
          - type: string
        method: false
      text:
        args:
          - type: string
        method: false
      varchar:
        args:
          - type: string
          - type: number
            required: false
        method: false
      char:
        args:
          - type: string
          - type: number
            required: false
        method: false
      boolean:
        args:
          - type: string
        method: false
      timestamp:
        args:
          - type: string
        method: false
      timestamptz:
        args:
          - type: string
        method: false
      date:
        args:
          - type: string
        method: false
      time:
        args:
          - type: string
        method: false
      uuid:
        args:
          - type: string
        method: false
      json:
        args:
          - type: string
        method: false
      jsonb:
        args:
          - type: string
        method: false
      numeric:
        args:
          - type: string
          - type: number
            required: false
          - type: number
            required: false
        method: false
      real:
        args:
          - type: string
        method: false
      double_precision:
        args:
          - type: string
        method: false
      bytea:
        args:
          - type: string
        method: false
      inet:
        args:
          - type: string
        method: false
      cidr:
        args:
          - type: string
        method: false
      enum_type:
        args:
          - type: string
          - type: string
        method: false
      array:
        args:
          - type: string
          - type: string
        method: false

  IndexBuilder:
    property: read-only
    struct:
      new:
        args:
          - type: string
        method: false

  # Enum constants
  ReferenceAction:
    property: read-only
    struct:
      NoAction:
        property: read-only
      Restrict:
        property: read-only
      Cascade:
        property: read-only
      SetNull:
        property: read-only
      SetDefault:
        property: read-only

  IndexMethod:
    property: read-only
    struct:
      BTree:
        property: read-only
      Hash:
        property: read-only
      Gist:
        property: read-only
      SpGist:
        property: read-only
      Gin:
        property: read-only
      Brin:
        property: read-only
"#;
