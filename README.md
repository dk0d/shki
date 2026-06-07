<div align="center">
<img src="/assets/shki-logo.png" alt="shki-logo" style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</div>

> [!WARNING]
> `shki` is still a work in progress. Declarative Schema support is active, but some deeper diff/render coverage and adoption workflows are still being built.

# shki

`shki` manages database schema change by comparing an intended database shape with recorded schema history and producing migration artifacts.

The current direction is SQL-first and Drizzle-inspired:

- You author a **Declarative Schema** in SQL.
- `shki` compiles it in a disposable **Shadow Database**.
- The resulting **Snapshot** is compared with the latest recorded Snapshot from the **Journal**.
- `shki diff` previews the **Migration Plan**.
- `shki generate` writes migration SQL, a new Snapshot, and a Journal entry.
- `shki migrate` applies migration artifacts to the live database.

## Supported Dialects

| Workflow                                 | PostgreSQL | MySQL | SQLite  |
| ---------------------------------------- | ---------- | ----- | ------- |
| Apply/status/down migration runner       | yes        | yes   | yes     |
| Custom Migration creation                | yes        | yes   | yes     |
| Dump live shape as SQL/JSON              | yes        | yes   | yes     |
| Dump live shape as Directory Schema      | yes        | yes   | yes     |
| Declarative Schema compile/diff/generate | yes        | no    | planned |
| Rich Catalog introspection coverage      | strongest  | basic | basic   |

Declarative Schema generation is PostgreSQL-focused for now. The migration runner remains dialect-aware for PostgreSQL, MySQL, and SQLite.

## Installation

```bash
cargo install --git https://github.com/dk0d/shki
```

Or locally:

```bash
git clone https://github.com/dk0d/shki
cd shki
cargo install --path .
```

## Quick Start: Declarative Schema

1. Initialize a project.

```bash
shki init db --dialect postgres
```

2. Configure your live database URL.

```bash
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
```

3. Create a Declarative Schema file.

```sql
-- db/schema
CREATE TABLE users (
  id integer PRIMARY KEY,
  email text NOT NULL UNIQUE
);
```

4. Preview the Migration Plan.

```bash
shki diff
```

5. Generate migration artifacts.

```bash
shki generate create_users --with-down
```

6. Apply pending migrations.

```bash
shki migrate
```

## Shadow Database Configuration

Declarative Schema compilation requires PostgreSQL execution in a Shadow Database.

By default, `shki` uses managed embedded PostgreSQL. You can pin the embedded PostgreSQL major version on commands that compile a Declarative Schema:

```bash
shki generate create_users --pg-version 16
```

Supported embedded major versions are `14`, `15`, `16`, `17`, and `18`.

For CI, locked-down environments, or teams that want explicit provisioning, configure an external Shadow Database:

```bash
export SHKI_SHADOW_DATABASE_URL='postgres://user:pass@localhost:5432/shki_shadow'
```

The Shadow Database is disposable. `shki` resets user schemas before applying the Declarative Schema. `shadow_database_url` must not be the same as `database_url`.

## Directory Schemas

A Declarative Schema can be a single SQL file or a directory with a canonical `main.sql` entrypoint.

```text
schema/
  main.sql
  tables/
    users.sql
```

```sql
-- schema/main.sql
\i tables/users.sql
```

Only `\i` include directives are supported in v1. Include paths are resolved relative to the including file, and include cycles are rejected.

## Commands

Global options:

- `-c, --config <PATH>`: config file, default `shki.toml`
- `-l, --dialect <postgres|mysql|sqlite>`: database dialect
- `-u, --database-url <URL>`: live database URL, env fallback `DATABASE_URL`
- `-d, --dir <PATH>`: migration output/read directory
- `-v, --verbose`: verbose output
- `--no-color`: disable color output

Command-scoped options:

- `diff`, `generate`, and `codegen` accept `--shadow-database-url <URL>` and `--pg-version <14|15|16|17|18>`.
- `create`, `generate`, `migrate`, `status`, and `down` accept migration options such as `--table <NAME>`, `--prefix <index|timestamp|unix>`, and `--generate-down` where applicable.
- `codegen` accepts codegen options such as `--output <PATH>`, `--format <single|singlemodule|modules>`, `--serde`, `--sqlx`, and `--no-sqlx`.

| Command                    | Alias  | Purpose                                                      |
| -------------------------- | ------ | ------------------------------------------------------------ |
| `config`                   | `conf` | Print the effective configuration                            |
| `init [path]`              | `i`    | Initialize a project directory                               |
| `dump`                     | -      | Export live database shape as SQL, JSON, or Directory Schema |
| `diff`                     | -      | Compile Declarative Schema and preview the Migration Plan    |
| `generate <name>`          | `gen`  | Generate schema-derived migration artifacts and a Snapshot   |
| `generate <name> --custom` | `gen`  | Create a Custom Migration                                    |
| `create <name>`            | `new`  | Create a Custom Migration for manual SQL editing             |
| `migrate`                  | `up`   | Apply pending migrations                                     |
| `status`                   | `s`    | Show migration status and checksum issues                    |
| `down [count]`             | -      | Apply Down Migrations for local rollback                     |
| `codegen`                  | `code` | Generate Rust, TypeScript, or Protobuf code from schema shape |
| `drop [migration]`         | -      | Remove a local migration, Down Migration, Snapshot, and Journal entry |

## Usage Patterns

### Preview Declarative Schema Changes

```bash
shki diff
```

`diff` compiles the Declarative Schema, compares it to the latest schema Snapshot in the Journal, and prints a preview of the Migration Plan. The preview summarizes object-level changes and rename candidates. It does not print generated SQL and does not write files.

### Generate A Schema-Derived Migration

```bash
shki generate add_users_table
```

`generate` writes:

- `migrations/<migration>.sql`
- `migrations/_meta/<migration>.snapshot.json`
- `migrations/_meta/_journal.json`

Use `--with-down` or `migrations.generate_down = true` to write a Down Migration:

```bash
shki generate add_users_table --with-down
```

If Shki detects possible renames, `generate` prompts before rendering the migration. Choosing a rename replaces drop/create statements with rename statements where supported.

### Create A Custom Migration

Use Custom Migrations for changes outside schema-shape planning, such as data backfills or operational SQL.

```bash
shki create backfill_user_emails --with-down
```

or:

```bash
shki generate backfill_user_emails --custom
```

Seed a Custom Migration with inline SQL:

```bash
shki create add_users_index \
  --sql 'CREATE INDEX idx_users_email ON users(email);'
```

Seed a Custom Migration from a file:

```bash
shki create add_audit_table --sql-file ./sql/add_audit_table.sql
```

Custom Migrations are executable artifacts. They are recorded in the Journal but do not create schema Snapshots.

### Apply Migrations

```bash
shki migrate
```

`migrate` applies pending SQL files and records applied checksums in the live database migration table. It does not mutate local Snapshot files or the Journal.

### Roll Back During Development

```bash
shki down --dry-run
shki down 1
```

Down Migrations are optional and intended for local iteration. They are not a recommended production rollback strategy.

### Drop A Local Migration

```bash
shki drop 0003_add_users
```

`drop` removes the selected local migration file, matching Down Migration, generated Snapshot, and Journal entry. Pending named drops are non-interactive; dropping an already-applied migration requires confirmation.

### Dump A Live Database

Export the live database shape as SQL:

```bash
shki dump
```

Export JSON Snapshot shape:

```bash
shki dump --format json --output snapshot.json
```

Export a Directory Schema:

```bash
shki dump --dirs --output schema
```

Preview Directory Schema output without writing files:

```bash
shki dump --dirs
```

Directory mode writes `main.sql`, top-level `extensions/`, and schema-scoped object directories where supported.

### Generate Code

By default, `codegen` compiles the current Declarative Schema through the Shadow Database and generates code from that current schema shape:

```bash
shki codegen --output src/schema rust
shki codegen --output src/schema typescript
shki codegen --output proto protobuf
```

Use `--source` to generate from a specific Snapshot JSON file, SQL Declarative Schema file, or Directory Schema:

```bash
shki codegen --source migrations/_meta/0000_create_users.snapshot.json --output src/schema rust
shki codegen --source migrations/_meta/0000_create_users.snapshot.json --output src/schema typescript
shki codegen --source migrations/_meta/0000_create_users.snapshot.json --output proto protobuf
```

Codegen supports three output modes:

- `single`: one generated file containing all generated types.
- `singlemodule`: one module directory with one generated file per type and a generated `mod.rs`.
- `modules`: one module directory per generated type, intended for projects that keep hand-written impl files next to generated definitions.

Configure codegen in `[codegen]`:

```toml
[codegen]
output = "src/schema"
format = "singlemodule"
serde = true
sqlx = true
struct_pattern = "{}Row"
enum_pattern = "Db{}"
include_tables = ["users", "orders"]
exclude_tables = ["audit_log"]
impl_file_name = "impl"

struct_derives = ["Debug", "Clone", "sqlx::FromRow"]
struct_attributes = ["#[allow(dead_code)]"]
enum_derives = ["Debug", "Clone", "PartialEq", "sqlx::Type"]
enum_attributes = ['#[serde(rename_all = "snake_case")]']

[codegen.struct_renames]
users = "Account"

[codegen.enum_renames]
user_status = "AccountStatus"

[codegen.type_overrides]
jsonb = "serde_json::Value"
"public.money" = "rust_decimal::Decimal"
```

Codegen options:

| Option | Purpose |
| ------ | ------- |
| `output` | Default output path when `--output` is not provided. Relative paths resolve from `root`. |
| `format` | Output layout: `single`, `singlemodule`, or `modules`. |
| `struct_derives` | Replaces the default derives attached to generated structs. |
| `struct_attributes` | Extra raw attributes added above generated structs. |
| `enum_derives` | Replaces the default derives attached to generated enums. |
| `enum_attributes` | Extra raw attributes added above generated enums. |
| `struct_renames` | Exact table-name to generated struct-name overrides. These apply before `struct_pattern`. |
| `struct_pattern` | Pattern for generated struct names. `{}` is replaced with the resolved base name. For table `users`, the base is `User`; pattern `{}Row` produces `UserRow`. |
| `enum_renames` | Exact enum-name to generated enum-name overrides. These apply before `enum_pattern`. |
| `enum_pattern` | Pattern for generated enum names. `{}` is replaced with the resolved base name. For enum `user_status`, the base is `UserStatus`; pattern `Db{}` produces `DbUserStatus`. |
| `type_overrides` | SQL type to generated type overrides. Built-in types use lowercase keys like `jsonb`; custom PostgreSQL types may use schema-qualified keys like `public.money`. |
| `serde` | Adds serde support to generated Rust structs/enums. |
| `sqlx` | Controls sqlx derives in generated Rust output. Defaults to `true`. |
| `include_tables` | If non-empty, only listed table names are generated. |
| `exclude_tables` | Listed table names are skipped. Applied after `include_tables`. |
| `verbose` | Prints generated code to stdout as well as writing files. |
| `impl_file_name` | File name stem for hand-written impl files in `modules` mode. |

Name resolution order is: explicit rename, default casing, then pattern. Struct defaults singularize table names and use PascalCase, so `users` becomes `User`. Enum defaults use PascalCase, so `user_status` becomes `UserStatus`.

## Configuration

Set config in `shki.toml`, environment variables, or CLI flags. If `root` is omitted, relative paths resolve from the directory containing the config file. If `root` is set, relative paths such as `schema`, `out`, dump outputs, and codegen outputs resolve from `root`.

Example PostgreSQL Declarative Schema config:

```toml
root = "db"
dialect = "postgres"
database_url = "postgres://user:pass@localhost:5432/mydb"
schema = "schema"
out = "migrations"
timeout_seconds = 2

# Optional. If omitted, Shki uses managed embedded PostgreSQL.
shadow_database_url = "postgres://user:pass@localhost:5432/shki_shadow"

# Optional. Supported: 14, 15, 16, 17, 18.
pg_version = 16

[migrations]
table = "__shki_migrations"
schema = "shki"
prefix = "index"
generate_down = false
```

MySQL and SQLite remain supported for migration-runner workflows:

```toml
# MySQL
dialect = "mysql"
database_url = "mysql://user:pass@localhost:3306/mydb"

# SQLite
dialect = "sqlite"
database_url = "sqlite://db/app.db"
```

Environment variables:

```bash
DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
SHKI_SHADOW_DATABASE_URL='postgres://user:pass@localhost:5432/shki_shadow'
SHKI_PG_VERSION=16
SHKI_MIGRATIONS__TABLE='__shki_migrations'
SHKI_MIGRATIONS__PREFIX='timestamp'
SHKI_MIGRATIONS__GENERATE_DOWN=true
```

`shki` also reads `.env` from the current working directory.

## Project Scope

Implemented or active:

- SQL Declarative Schema loading from a single file or Directory Schema.
- PostgreSQL Shadow Database compilation through embedded or external PostgreSQL.
- Snapshot and Journal history for generated schema migrations.
- `diff` previews based on Migration Plan summaries and rename candidates.
- `generate` for schema-derived migrations.
- Custom Migrations via `create` or `generate --custom`.
- `dump` for SQL, JSON, and Directory Schema export.
- `drop` for removing local migration artifacts and Journal entries.
- Migration runner, status, checksum validation, and Down Migrations.
- Code generation from current Declarative Schema or Snapshot JSON for Rust, TypeScript, and Protobuf.

Still in progress:

- Bootstrap workflows for adopting an existing database.
- Validation workflows for comparing Declarative Schema, Snapshot history, migration artifacts, and live database shape.
- Full diff/render semantics for every PostgreSQL Catalog object represented by Dump.
- Declarative Schema generation for MySQL and SQLite.

## Notes

### Snapshots And Journal

Snapshots record database shape. The Journal relates migrations to Snapshots and is the history index for generated artifacts.

Schema-derived generation uses the latest schema Snapshot from the Journal as the baseline. Custom Migrations can appear between schema-derived migrations without breaking that chain.

### Checksum Validation

When `status` or `migrate` can connect to the database, `shki` validates stored checksums for applied migrations and reports if a migration file has changed since it was applied.

### Safety

Always use a disposable Shadow Database. `shki` resets user schemas in the configured Shadow Database before compilation.

## Contributing

Issues and pull requests are welcome.

## License

MIT, see [LICENSE](LICENSE).
