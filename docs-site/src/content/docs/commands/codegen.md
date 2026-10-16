---
title: shki codegen
description: Generate Rust, TypeScript, and Protobuf types from the schema shape.
---

```bash
shki codegen [OPTIONS] <rust|typescript|protobuf>
```

Alias: `shki code`; language aliases `rs`, `ts`, `proto`.

`codegen` turns a schema shape into types. By default it compiles the current
Declarative Schema through the Shadow Database and generates from that, so the
types always match the schema you have committed — no live database needed.

## Options

CLI flags override the corresponding `shki.toml` keys below. See
[Configuration](#configuration) for defaults and config-only settings, and the
[CLI reference](../../reference/cli/#global-options) for global options.

| Flag                                          | TOML key                  | Purpose                                                                                      |
| --------------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------- |
| `-o, --output <PATH>`                         | `codegen.output`          | Output directory. In Rust `file` mode, writes `models.rs` inside it.                         |
| `-f, --format <file\|module\|modules>`        | `codegen.format`          | Output layout. Default `file`.                                                               |
| `-s, --source <PATH>`                         | —                         | Generate from a Snapshot JSON, SQL schema file, or Directory Schema instead.                 |
| `--serde[=<bool>]`                            | `codegen.serde`           | Serde derives and rename attributes. Bare flag enables, `=false` disables.                   |
| `--sqlx[=<bool>]`                             | `codegen.sqlx`            | `sqlx::FromRow`/`sqlx::Type` derives. Defaults to on; `=false` disables.                     |
| `--date-time-crate <chrono\|jiff\|jiff-sqlx>` | `codegen.date_time_crate` | Rust date/time library. Default `chrono`; use `jiff-sqlx` for SQLx-compatible Jiff wrappers. |
| `--preview`                                   | —                         | Print the generated code without writing files.                                              |
| `--shadow-database-url <URL>`                 | `shadow_database_url`     | External Shadow Database (top-level config key).                                             |
| `--pg-version <14…18>`                        | `pg_version`              | Embedded PostgreSQL major version (top-level config key). Default `18`.                      |

For Shadow Database setup, see [Declarative Schema](../../guides/declarative-schema/#shadow-database-configuration).

## Examples

```bash
shki codegen --output src/schema rust
shki codegen --output src/schema --format modules rs
shki codegen --output web/src/db typescript
shki codegen --output proto proto
shki codegen --preview rust
```

Use `--source` to generate from a specific Snapshot JSON file, SQL Declarative
Schema file, or Directory Schema instead of compiling the current one:

```bash
shki codegen --source migrations/_meta/0000_create_users.snapshot.json --output src/schema rust
```

`--preview` prints the result without writing files.

## Output layouts

`--format` / `[codegen] format`:

| Mode      | Layout                                                                                                     |
| --------- | ---------------------------------------------------------------------------------------------------------- |
| `file`    | One file with every struct and enum, e.g. `out/models.rs`. Default. Always overwritten.                    |
| `module`  | One module directory, one file per type plus a generated `mod.rs`. Always overwritten.                     |
| `modules` | One directory per type: `_def.rs` holds the generated definition, `mod.rs` mounts it and is yours to edit. |

`modules` is the mode to pick when you want hand-written `impl` blocks next to
generated types: `_def.rs` is always regenerated, `mod.rs` is only created when
missing, so anything you add there survives regeneration.

## Naming

Name resolution order is: explicit rename, default casing, then pattern. Struct
defaults singularize table names and use PascalCase, so `users` becomes `User`.
Enum defaults use PascalCase, so `user_status` becomes `UserStatus`. A pattern
then wraps that base name — `{}Row` turns `User` into `UserRow`.

## Configuration

Everything below lives in `[codegen]` in `shki.toml`. CLI flags override it;
see [configuration precedence and path resolution](../../reference/configuration/)
for environment overrides and `root`. Naming, derives, attributes, type
overrides, and table filters have no command-specific flags.

```toml
[codegen]
output = "src/schema"
format = "module"
serde = true
sqlx = true
date_time_crate = "chrono"
struct_pattern = "{}Row"
enum_pattern = "Db{}"
include_tables = ["users", "orders"]
exclude_tables = ["audit_log"]

struct_derives = ["Debug", "Clone"]
struct_attributes = ["allow(dead_code)"]
enum_derives = ["Debug", "Clone", "PartialEq"]
enum_attributes = ["non_exhaustive"]

[codegen.struct_renames]
users = "Account"

[codegen.enum_renames]
user_status = "AccountStatus"

[codegen.type_overrides]
jsonb = "serde_json::Value"
"public.money" = "rust_decimal::Decimal"
```

| Option              | Default                           | Purpose                                                                                                                                                          |
| ------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `output`            | –                                 | Default output path when `--output` is not provided. Relative paths resolve from `root`.                                                                         |
| `format`            | `file`                            | Output layout: `file`, `module`, or `modules`.                                                                                                                   |
| `serde`             | `false`                           | Injects `serde::Serialize`/`Deserialize` derives and `#[serde(rename)]` attributes. Kept out of the derive lists so it can be toggled.                           |
| `sqlx`              | `true`                            | Injects `sqlx::FromRow` (structs) / `sqlx::Type` (enums) derives and `#[sqlx(...)]` attributes. Set `false` for plain types with no sqlx coupling.               |
| `date_time_crate`   | `chrono`                          | Rust temporal types: `chrono`, `jiff`, or `jiff-sqlx`. Explicit `type_overrides` take precedence.                                                                |
| `struct_derives`    | `["Debug", "Clone"]`              | Replaces the default derives attached to generated structs.                                                                                                      |
| `struct_attributes` | –                                 | Extra attribute bodies added above generated structs; shki supplies `#[...]`.                                                                                    |
| `enum_derives`      | `["Debug", "Clone", "PartialEq"]` | Replaces the default derives attached to generated enums.                                                                                                        |
| `enum_attributes`   | –                                 | Extra attribute bodies added above generated enums; shki supplies `#[...]`.                                                                                      |
| `struct_renames`    | –                                 | Exact table-name to generated struct-name overrides. Applied before `struct_pattern`.                                                                            |
| `struct_pattern`    | –                                 | Pattern for generated struct names; `{}` is the resolved base name. For table `users` the base is `User`, so `{}Row` produces `UserRow`.                         |
| `enum_renames`      | –                                 | Exact enum-name to generated enum-name overrides. Applied before `enum_pattern`.                                                                                 |
| `enum_pattern`      | –                                 | Pattern for generated enum names; `{}` is the resolved base name. For enum `user_status` the base is `UserStatus`, so `Db{}` produces `DbUserStatus`.            |
| `type_overrides`    | –                                 | SQL type to generated type overrides. Built-in types use lowercase keys like `jsonb`; custom PostgreSQL types may use schema-qualified keys like `public.money`. |
| `include_tables`    | all                               | If non-empty, only listed table names are generated.                                                                                                             |
| `exclude_tables`    | –                                 | Listed table names are skipped. Applied after `include_tables`.                                                                                                  |

The same type mapping and naming config is reused by
[`shki queries`](../../commands/queries/), so generated query functions return the
structs and enums defined here rather than parallel copies.

## Configuration examples

These examples use the Rust target. Save this schema as `schema.sql`:

```sql
CREATE TYPE user_status AS ENUM ('active', 'on_hold');

CREATE TABLE users (
    id bigint PRIMARY KEY,
    "type" text NOT NULL,
    status user_status NOT NULL,
    created_at timestamptz NOT NULL,
    metadata jsonb
);

CREATE TABLE orders (id bigint PRIMARY KEY);
CREATE TABLE audit_log (id bigint PRIMARY KEY);
```

Try each TOML block independently in `shki.toml`, using defaults for settings
not shown, then run:

```bash
shki codegen --source schema.sql --preview rust
```

The Rust excerpts below show the affected declarations; unrelated fields,
attributes, imports, and generated headers are omitted.

### `output` and `format`

```toml
[codegen]
output = "src/schema"
format = "file"
```

Run without `--preview` to write files:

```bash
shki codegen --source schema.sql rust
```

With `root` set to the project directory, this writes
`src/schema/models.rs`, containing `User`, `Order`, `AuditLog`, and `UserStatus`.
`output` is a directory, including in `file` mode. Changing it to
`"src/db"` writes `src/db/models.rs` instead. Without `output`, generation is
skipped unless `--preview` is used.

Keep `output = "src/schema"` and change `format` to see these layouts:

| Setting              | Files under `src/schema/`                                                                                                                 |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `format = "file"`    | `models.rs` containing every type.                                                                                                        |
| `format = "module"`  | `mod.rs`, `user.rs`, `order.rs`, `audit_log.rs`, `user_status.rs`.                                                                        |
| `format = "modules"` | Top-level `mod.rs`, plus `user/{_def.rs,mod.rs}`, `order/{_def.rs,mod.rs}`, `audit_log/{_def.rs,mod.rs}`, `user_status/{_def.rs,mod.rs}`. |

In `modules` mode, each per-type `mod.rs` initially contains:

```rust
mod _def;
pub use _def::*;
```

Those per-type `mod.rs` files survive regeneration; `_def.rs` and the top-level
`mod.rs` are regenerated. Module filenames follow the generated type names, so
renaming `User` to `Account` also changes `user.rs` to `account.rs` in `module` mode.

### `serde`

```toml
[codegen]
serde = true
sqlx = false
```

This adds Serde derives and rename attributes for the Rust keyword field and
database enum values:

```rust
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct User {
    #[serde(rename = "type")]
    pub r#type: String,
    // Other fields omitted.
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum UserStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "on_hold")]
    OnHold,
}
```

For example, `UserStatus::OnHold` serializes as `"on_hold"`. With `serde = false`
(the default), these automatically added Serde derives and attributes are absent.

### `sqlx`

```toml
[codegen]
sqlx = true
```

This is the default. It produces SQLx derives and database-name mappings:

```rust
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    #[sqlx(rename = "type")]
    pub r#type: String,
    // Other fields omitted.
}

#[derive(Clone, Debug, PartialEq, sqlx::Type)]
#[sqlx(type_name = "user_status")]
pub enum UserStatus {
    #[sqlx(rename = "active")]
    Active,
    #[sqlx(rename = "on_hold")]
    OnHold,
}
```

Setting `sqlx = false` removes the automatically added `sqlx::FromRow`,
`sqlx::Type`, and `#[sqlx(...)]` attributes. The base derives remain:
`#[derive(Clone, Debug)]` on `User` and
`#[derive(Clone, Debug, PartialEq)]` on `UserStatus`.

### `date_time_crate`

```toml
[codegen]
date_time_crate = "jiff"
sqlx = false
```

The `created_at timestamptz NOT NULL` column becomes:

```rust
pub created_at: jiff::Timestamp,
```

Changing `date_time_crate` changes that field's type:

| Setting                         | Generated field                                  |
| ------------------------------- | ------------------------------------------------ |
| `date_time_crate = "chrono"`    | `pub created_at: chrono::DateTime<chrono::Utc>,` |
| `date_time_crate = "jiff"`      | `pub created_at: jiff::Timestamp,`               |
| `date_time_crate = "jiff-sqlx"` | `pub created_at: jiff_sqlx::Timestamp,`          |

Use `jiff-sqlx` when enabling SQLx derives. See [Jiff date/time types](#jiff-datetime-types)
for the other temporal mappings and required application dependencies.

### `struct_derives` and `enum_derives`

```toml
[codegen]
struct_derives = ["Debug", "PartialEq"]
enum_derives = ["Debug", "PartialEq", "Eq"]
```

Non-empty lists **replace** the defaults; they do not append to them. In this case,
`Clone` disappears and the enabled-by-default SQLx derives are still injected:

```rust
#[derive(Debug, PartialEq, sqlx::FromRow)]
pub struct User {
    // Fields omitted.
}

#[derive(Debug, Eq, PartialEq, sqlx::Type)]
pub enum UserStatus {
    // Variants and SQLx attributes omitted.
}
```

Derives are sorted and deduplicated. Empty lists currently fall back to the
default derives. Use the `serde` and `sqlx` toggles for their derives so they
can be disabled independently.

### `struct_attributes` and `enum_attributes`

```toml
[codegen]
struct_attributes = ["allow(dead_code)"]
enum_attributes = ["non_exhaustive"]
```

shki wraps each string in `#[...]`; write the attribute body, **without** those
outer delimiters:

```rust
#[derive(Clone, Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct User {
    // Fields omitted.
}

#[derive(Clone, Debug, PartialEq, sqlx::Type)]
#[non_exhaustive]
#[sqlx(type_name = "user_status")]
pub enum UserStatus {
    // Variants omitted.
}
```

`struct_attributes` applies to every generated struct, including `Order` and
`AuditLog`; `enum_attributes` applies to every generated enum. These attributes
are emitted in addition to derives and automatic Serde/SQLx attributes.

### `struct_renames` and `enum_renames`

```toml
[codegen.struct_renames]
users = "Account"

[codegen.enum_renames]
user_status = "AccountStatus"
```

The table's struct, the enum, and fields referring to that enum use the new names:

```rust
pub enum AccountStatus {
    Active,
    OnHold,
}

pub struct Account {
    pub status: AccountStatus,
    // Other fields omitted.
}
```

`Order` and `AuditLog` keep their default names. Database names stay unchanged:
SQLx still gets `#[sqlx(type_name = "user_status")]` on `AccountStatus`.

### `struct_pattern` and `enum_pattern`

```toml
[codegen]
struct_pattern = "{}Row"
enum_pattern = "Db{}"
```

The patterns wrap every resolved base name, including references in fields:

```rust
pub enum DbUserStatus {
    Active,
    OnHold,
}

pub struct UserRow {
    pub status: DbUserStatus,
    // Other fields omitted.
}
```

`Order` becomes `OrderRow` and `AuditLog` becomes `AuditLogRow`. If the rename
maps from the previous example are also set, the patterns apply **after** those
renames: `users` becomes `AccountRow`, `user_status` becomes `DbAccountStatus`,
and the field is `pub status: DbAccountStatus,`.

### `type_overrides`

```toml
[codegen.type_overrides]
jsonb = "sqlx::types::Json<serde_json::Value>"
```

Without this setting, the nullable `metadata jsonb` column generates
`pub metadata: Option<serde_json::Value>,`. With it:

```rust
pub metadata: Option<sqlx::types::Json<serde_json::Value>>,
```

Nullability still adds `Option<...>`. An array of this SQL type uses
`Vec<sqlx::types::Json<serde_json::Value>>` (wrapped in `Option` if nullable).
Overrides take precedence over the built-in mappings, including `date_time_crate`.
The consuming application must provide the chosen type and its required traits.

### `include_tables` and `exclude_tables`

```toml
[codegen]
include_tables = ["users", "orders"]
exclude_tables = ["orders"]
```

Only `User` is generated from the tables: `audit_log` is outside the include
list, and `orders` is removed by the exclude list. `UserStatus` is still generated.

| Settings in `[codegen]`                                                  | Generated table structs     |
| ------------------------------------------------------------------------ | --------------------------- |
| Neither list set (or both `[]`)                                          | `User`, `Order`, `AuditLog` |
| `include_tables = ["users", "orders"]`                                   | `User`, `Order`             |
| `exclude_tables = ["audit_log"]`                                         | `User`, `Order`             |
| `include_tables = ["users", "orders"]` and `exclude_tables = ["orders"]` | `User`                      |

Filters match original table names (`users`, not `User` or a renamed `Account`).
They do not filter enum or composite type definitions.

## Jiff date/time types

For native Jiff types:

```toml
[codegen]
date_time_crate = "jiff"
sqlx = false
```

Or run `shki codegen --date-time-crate jiff --sqlx=false --output src/schema rust`.
Add `jiff = "0.2"` to the consuming application's dependencies; enable its
`serde` feature if generating Serde derives.

| SQL type                        | `jiff`                  | `jiff-sqlx`            |
| ------------------------------- | ----------------------- | ---------------------- |
| `date`                          | `jiff::civil::Date`     | `jiff_sqlx::Date`      |
| `time` (without time zone)      | `jiff::civil::Time`     | `jiff_sqlx::Time`      |
| `timestamp` (without time zone) | `jiff::civil::DateTime` | `jiff_sqlx::DateTime`  |
| `timestamp with time zone`      | `jiff::Timestamp`       | `jiff_sqlx::Timestamp` |

Nullable columns and arrays retain their `Option<T>` and `Vec<T>` wrappers.
Timezone-aware timestamps represent instants, not named time zones. Other
mappings, including intervals and ranges, retain their existing defaults.

### With SQLx and typed queries

SQLx does not implement its database traits directly for native Jiff types.
Use `date_time_crate = "jiff-sqlx"` with SQLx derives or typed queries:

```toml
# shki.toml
[codegen]
date_time_crate = "jiff-sqlx"
```

Add the integration crate to the consuming application's `Cargo.toml`:

```toml
[dependencies]
jiff = "0.2"
jiff-sqlx = { version = "0.2", features = ["postgres"] }
sqlx = { version = "0.9", features = ["runtime-tokio", "postgres", "macros"] }
```

`jiff-sqlx` supports PostgreSQL and SQLite (select its `sqlite` feature for
SQLite), but not MySQL. Version `0.2` targets SQLx `0.9`; applications using
SQLx `0.8` need `jiff-sqlx` `0.1`. Convert wrappers to native Jiff values with
`.to_jiff()` and native values to wrappers with `.into()`.

### SQLx and Serde on the same model

Enable both in `shki.toml`:

```toml
[codegen]
date_time_crate = "jiff-sqlx"
serde = true
sqlx = true
```

The wrappers do not implement Serde traits themselves. shki generates
field-level `serde_with::FromInto` adapters that serialize and deserialize
through native Jiff types, keeping the SQLx-compatible public fields:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct Event {
    #[serde(with = "serde_with::As::<serde_with::FromInto<jiff::Timestamp>>")]
    pub created_at: jiff_sqlx::Timestamp,
}
```

Alongside the SQLx dependencies above, use these dependencies in your application:

```toml
jiff = { version = "0.2", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_with = { version = "3", default-features = false, features = ["std"] }
```

Adapters also cover nullable fields and arrays. Missing nullable fields deserialize
as `None`, and JSON uses Jiff's native string representation. Models with Serde
disabled, native Jiff fields, or unrelated custom type overrides need no adapters.
