---
title: Code Generation
description: Generate Rust, TypeScript, and Protobuf types from the schema shape.
slug: 0.10.7/guides/codegen
---

`codegen` turns a schema shape into types. By default it compiles the current
Declarative Schema through the Shadow Database and generates from that, so the
types always match the schema you have committed — no live database needed.

```bash
shki codegen --output src/schema rust
shki codegen --output src/schema typescript
shki codegen --output proto protobuf
```

Language subcommands accept aliases: `rs`, `ts`, `proto`.

Use `--source` to generate from a specific Snapshot JSON file, SQL Declarative
Schema file, or Directory Schema instead of compiling the current one:

```bash
shki codegen --source migrations/_meta/0000_create_users.snapshot.json --output src/schema rust
```

`--preview` prints the result without writing files.

## Output modes

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

Everything below lives in `[codegen]` in `shki.toml`. CLI flags override it.

```toml
[codegen]
output = "src/schema"
format = "module"
serde = true
sqlx = true
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
[typed queries](../../guides/queries/), so generated query functions return the
structs and enums defined here rather than parallel copies.
