---
title: shki codegen
description: Generate Rust, TypeScript, or Protobuf types from the schema shape.
---

```bash
shki codegen [OPTIONS] <rust|typescript|protobuf>
```

Alias: `shki code`; language aliases `rs`, `ts`, `proto`. By default it compiles
the current Declarative Schema in the Shadow Database and generates from that
shape, so the types match what you have committed — no live database involved.

## Options

| Flag                                   | Purpose                                                                       |
| -------------------------------------- | ----------------------------------------------------------------------------- |
| `-o, --output <PATH>`                  | Output directory (or file, in `file` mode). Falls back to `[codegen] output`. |
| `-f, --format <file\|module\|modules>` | Output layout. Default `file`.                                                |
| `-s, --source <PATH>`                  | Generate from a Snapshot JSON, SQL schema file, or Directory Schema instead.  |
| `--serde[=<bool>]`                     | Serde derives and rename attributes. Bare flag enables, `=false` disables.    |
| `--sqlx[=<bool>]`                      | `sqlx::FromRow`/`sqlx::Type` derives. Defaults to on.                         |
| `--preview`                            | Print the generated code without writing files.                               |
| `--shadow-database-url <URL>`          | External Shadow Database.                                                     |
| `--pg-version <14…18>`                 | Embedded PostgreSQL major version.                                            |

## Examples

```bash
shki codegen --output src/schema rust
shki codegen --output src/schema --format modules rs
shki codegen --output web/src/db.ts typescript
shki codegen --output proto proto
shki codegen --source migrations/_meta/0000_create_users.snapshot.json -o src/schema rust
shki codegen --preview rust                     # look before writing
```

## Output layouts

| Mode      | Layout                                                                                               |
| --------- | ---------------------------------------------------------------------------------------------------- |
| `file`    | One file with every type. Always overwritten.                                                        |
| `module`  | A module directory, one file per type plus `mod.rs`. Always overwritten.                             |
| `modules` | One directory per type: generated `_def.rs` (always overwritten) and a `mod.rs` you can edit safely. |

Naming, derives, type overrides, and table filters live in `[codegen]` in
`shki.toml`.

**Full details: [Code Generation guide](/shki/guides/codegen/).**

See also: [`shki queries`](/shki/commands/queries/) for typed query functions
that reuse these generated types.
