---
title: CLI Reference
description: Global options and an index of every shki command.
---

Run `shki <command> --help` for the authoritative list of flags. Each command
has its own page under [Commands](/shki/commands/config/); this page covers what
they share.

## Global options

Available on every command:

| Flag                       | Purpose                                                           |
| -------------------------- | ----------------------------------------------------------------- |
| `-c, --config <PATH>`      | Config file. Default `shki.toml`.                                 |
| `-l, --dialect <DIALECT>`  | `postgres`, `mysql`, or `sqlite`.                                 |
| `-u, --database-url <URL>` | Live database URL. Env fallback `DATABASE_URL`.                   |
| `-d, --migrations-dir <P>` | Directory migrations are written to and read from (also `--dir`). |
| `-S, --schema <NAME>`      | Schema holding the migrations table (PostgreSQL).                 |
| `-v, --verbose`            | Verbose output.                                                   |
| `-n, --no-color`           | Disable color output.                                             |
| `-V, --version`            | Print version.                                                    |

## Shared option groups

Commands that compile a Declarative Schema — `diff`, `generate`, `codegen`,
`queries` — also accept:

| Flag                          | Purpose                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `--shadow-database-url <URL>` | External Shadow Database. Env fallback `SHKI_SHADOW_DATABASE_URL`.               |
| `--pg-version <14…18>`        | Embedded PostgreSQL major version. Default `18`. Env fallback `SHKI_PG_VERSION`. |

Commands that read or write migrations — `create`, `generate`, `migrate`,
`status`, `down`, `bootstrap`, `adopt` — also accept:

| Flag                                | Purpose                                             |
| ----------------------------------- | --------------------------------------------------- |
| `-T, --table <NAME>`                | Migrations table name. Default `__shki_migrations`. |
| `--prefix <index\|timestamp\|unix>` | Migration file name prefix style. Default `index`.  |
| `--generate-down`                   | Generate Down Migrations alongside up migrations.   |

## Commands

| Command                                         | Alias      | Purpose                                                        |
| ----------------------------------------------- | ---------- | -------------------------------------------------------------- |
| [`config`](/shki/commands/config/)              | `conf`     | Print the effective configuration                              |
| [`init [path]`](/shki/commands/init/)           | `i`        | Scaffold a project: config, schema, migrations metadata        |
| [`diff`](/shki/commands/diff/)                  | –          | Compile the Declarative Schema and preview the Migration Plan  |
| [`generate <name>`](/shki/commands/generate/)   | `gen`      | Write migration SQL, a Snapshot, and a Journal entry           |
| [`create <name>`](/shki/commands/create/)       | `new`      | Create a Custom Migration for hand-written SQL                 |
| [`migrate [mode]`](/shki/commands/migrate/)     | `up`       | Apply pending migrations (`all`, `steps <N>`, `to <NAME>`)     |
| [`status`](/shki/commands/status/)              | `s`        | List migrations, applied state, and checksum problems          |
| [`down [count]`](/shki/commands/down/)          | –          | Roll back applied migrations using Down Migrations             |
| [`drop [migration]`](/shki/commands/drop/)      | –          | Remove a local migration and its artifacts                     |
| [`dump`](/shki/commands/dump/)                  | –          | Export live database shape as SQL, JSON, or a Directory Schema |
| [`bootstrap [name]`](/shki/commands/bootstrap/) | `strap`    | Author a baseline migration from an existing database          |
| [`adopt [name]`](/shki/commands/adopt/)         | `baseline` | Adopt an existing database at a committed baseline             |
| [`codegen <lang>`](/shki/commands/codegen/)     | `code`     | Generate Rust, TypeScript, or Protobuf types from schema shape |
| [`queries`](/shki/commands/queries/)            | `q`        | Generate type-safe Rust query functions from annotated SQL     |
