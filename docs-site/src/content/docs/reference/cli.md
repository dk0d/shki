---
title: CLI Reference
description: Every shki command, alias, and flag.
---

Run `shki <command> --help` for the authoritative list; this page mirrors it.

## Global options

Available on every command:

| Flag                        | Purpose                                                                 |
| --------------------------- | ----------------------------------------------------------------------- |
| `-c, --config <PATH>`       | Config file. Default `shki.toml`.                                       |
| `-l, --dialect <DIALECT>`   | `postgres`, `mysql`, or `sqlite`.                                       |
| `-u, --database-url <URL>`  | Live database URL. Env fallback `DATABASE_URL`.                          |
| `-d, --migrations-dir <P>`  | Directory migrations are written to and read from (also `--dir`).       |
| `-S, --schema <NAME>`       | Schema holding the migrations table (PostgreSQL).                       |
| `-v, --verbose`             | Verbose output.                                                         |
| `-n, --no-color`            | Disable color output.                                                   |
| `-V, --version`             | Print version.                                                          |

Commands that compile a Declarative Schema (`diff`, `generate`, `codegen`,
`queries`) additionally accept:

| Flag                            | Purpose                                                                              |
| ------------------------------- | ------------------------------------------------------------------------------------ |
| `--shadow-database-url <URL>`   | External Shadow Database. Env fallback `SHKI_SHADOW_DATABASE_URL`.                    |
| `--pg-version <14…18>`          | Embedded PostgreSQL major version. Default `18`. Env fallback `SHKI_PG_VERSION`.      |

Commands that read or write migrations (`create`, `generate`, `migrate`,
`status`, `down`, `bootstrap`, `adopt`) additionally accept:

| Flag                                    | Purpose                                                    |
| --------------------------------------- | ------------------------------------------------------------ |
| `-T, --table <NAME>`                    | Migrations table name. Default `__shki_migrations`.         |
| `--prefix <index\|timestamp\|unix>`     | Migration file name prefix style. Default `index`.          |
| `--generate-down`                       | Generate Down Migrations alongside up migrations.           |

## Commands

| Command                    | Alias      | Purpose                                                                                                                    |
| -------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------- |
| `config`                   | `conf`     | Print the effective configuration                                                                                          |
| `init [path]`              | `i`        | Initialize a project directory with config, schema, migrations metadata, and Postgres editor config where applicable       |
| `dump`                     | -          | Export live database shape as SQL, JSON, or Directory Schema                                                               |
| `diff`                     | -          | Compile Declarative Schema and preview the Migration Plan                                                                  |
| `generate <name>`          | `gen`      | Generate schema-derived migration artifacts and a Snapshot                                                                 |
| `generate <name> --custom` | `gen`      | Create a Custom Migration                                                                                                  |
| `create <name>`            | `new`      | Create a Custom Migration for manual SQL editing                                                                           |
| `migrate [mode]`           | `up`       | Apply all pending migrations, a limited number of pending migrations, or through a named pending migration                 |
| `bootstrap [name]`         | `strap`    | Author an initial baseline migration from an existing database (writes artifacts only; never touches the target DB)        |
| `adopt [name]`             | `baseline` | Adopt an existing database at a committed baseline (validate, mark applied without executing), then apply newer migrations |
| `status`                   | `s`        | Show migration status and checksum issues                                                                                  |
| `down [count]`             | -          | Apply Down Migrations for local rollback                                                                                   |
| `codegen <lang>`           | `code`     | Generate Rust, TypeScript, or Protobuf code from schema shape                                                              |
| `queries`                  | `q`        | Generate type-safe Rust query functions from annotated PostgreSQL files                                                    |
| `drop [migration]`         | -          | Remove a local migration, Down Migration, Snapshot, and Journal entry                                                      |

### `init [path]`

Scaffolds a project in `path` (default `.`): `shki.toml`, `schema/main.sql`,
`migrations/_meta/`, and — for PostgreSQL — `postgres-language-server.jsonc`.
Takes `--dialect <postgres|mysql|sqlite>`.

### `diff`

Compiles the Declarative Schema in the Shadow Database and prints an
object-level preview of the Migration Plan, including rename candidates. Writes
nothing and prints no SQL.

### `generate <name>`

| Flag             | Purpose                                                    |
| ---------------- | ------------------------------------------------------------ |
| `--down`         | Also write a Down Migration.                                 |
| `--custom`       | Create a Custom Migration instead of a schema-derived one.   |

Writes `migrations/<migration>.sql`, `migrations/_meta/<migration>.snapshot.json`,
and updates `_journal.json`. Prompts when rename candidates are detected.

### `create <name>`

| Flag                | Purpose                                             |
| ------------------- | ----------------------------------------------------- |
| `--with-down`       | Also create a `.down.sql` file.                      |
| `--sql <SQL>`       | Seed the migration with inline SQL.                  |
| `--sql-file <PATH>` | Seed the migration from a file.                      |
| `-e, --edit`        | Open the created file in `$EDITOR`.                  |

### `migrate [all|steps <N>|to <NAME>]`

| Flag     | Purpose                                     |
| -------- | --------------------------------------------- |
| `--dry`  | Show what would be applied, change nothing.  |

With no mode, applies every pending migration.

### `down [count]`

| Flag          | Purpose                                        |
| ------------- | ------------------------------------------------ |
| `--dry-run`   | Show what would be rolled back, change nothing. |

Rolls back `count` applied migrations (default 1) using their Down Migrations.

### `status`

Lists migrations, their applied state, and checksum mismatches when a database
connection is available.

### `dump`

| Flag                       | Purpose                                                  |
| -------------------------- | ---------------------------------------------------------- |
| `-f, --format <json\|sql>` | Output format. Default `sql`.                             |
| `--output <PATH>`          | Output file. Defaults to stdout.                          |
| `--dirs`                   | Emit a Directory Schema with `main.sql` as the entrypoint. |
| `--force`                  | Overwrite file collisions in directory mode.               |
| `--schema <NAME>`          | Schema to dump (PostgreSQL, default `public`).             |

### `bootstrap [name]`

| Flag              | Purpose                                                              |
| ----------------- | ---------------------------------------------------------------------- |
| `--dry-run`       | Show what would be generated without writing files.                    |
| `--force`         | Run even when migrations/Snapshots already exist locally.              |
| `--schema <NAME>` | Schema to bootstrap (PostgreSQL, default `public`).                    |

Name defaults to `bootstrap`. Only authors files; never writes to the database.

### `adopt [name]`

| Flag              | Purpose                                                                     |
| ----------------- | ----------------------------------------------------------------------------- |
| `--mark-only`     | Mark the baseline applied but do not apply newer pending migrations.          |
| `--force`         | Mark applied even if the live database differs from the baseline Snapshot.    |
| `--dry-run`       | Show what would be validated, marked, and applied without changing anything.  |
| `--schema <NAME>` | Schema to introspect for validation (PostgreSQL, default `public`).           |

Name defaults to the earliest schema migration.

### `codegen <rust|typescript|protobuf>`

Language subcommands alias to `rs`, `ts`, and `proto`.

| Flag                                    | Purpose                                                                  |
| --------------------------------------- | -------------------------------------------------------------------------- |
| `-o, --output <PATH>`                   | Output directory for generated code.                                       |
| `-f, --format <file\|module\|modules>`  | Output layout. Default `file`.                                             |
| `-s, --source <PATH>`                   | Generate from a schema file/directory or Snapshot JSON instead of compiling.|
| `--serde[=<bool>]`                      | Serde derives and rename attributes. Bare flag enables; `=false` disables.  |
| `--sqlx[=<bool>]`                       | `sqlx::FromRow`/`sqlx::Type` derives. Bare flag enables; `=false` disables. |
| `--preview`                             | Print the output without writing anything.                                 |

### `queries`

| Flag                                    | Purpose                                                                |
| --------------------------------------- | ------------------------------------------------------------------------ |
| `-s, --sources <PATH>`                  | Annotated `*.sql` file or directory. Default `<root>/queries`.           |
| `-o, --output <PATH>`                   | Output file for generated Rust. Prints to stdout when omitted.           |
| `-f, --format <file\|module\|modules>`  | Output layout.                                                          |
| `--models <PATH>`                       | Rust module path for schema types, e.g. `crate::models`. Usually derived.|
| `--preview`                             | Print the generated code without writing.                               |

### `drop [migration]`

| Flag            | Purpose                                                          |
| --------------- | ------------------------------------------------------------------ |
| `-f, --force`   | Skip database validation (won't check whether it was applied).    |

### `config`

Prints the effective configuration after merging `shki.toml`, environment
variables, and flags — the fastest way to see which values actually apply.
