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
| `-o, --output <PATH>`                         | `codegen.output`          | Output directory (or file, in `file` mode).                                                  |
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
shki codegen --output web/src/db.ts typescript
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
struct_attributes = ["#[allow(dead_code)]"]
enum_derives = ["Debug", "Clone", "PartialEq"]
enum_attributes = ['#[serde(rename_all = "snake_case")]']

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
| `struct_attributes` | –                                 | Extra raw attributes added above generated structs.                                                                                                              |
| `enum_derives`      | `["Debug", "Clone", "PartialEq"]` | Replaces the default derives attached to generated enums.                                                                                                        |
| `enum_attributes`   | –                                 | Extra raw attributes added above generated enums.                                                                                                                |
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
