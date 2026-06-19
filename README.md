<div align="center">
<img src="/assets/shki-logo.png" alt="shki-logo" style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</div>

> [!WARNING]
> `shki` is still a work in progress. Declarative Schema support is active, but some deeper diff/render coverage and validation workflows are still being built.

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

`init` creates a project layout like:

```text
db/
  shki.toml
  postgres-language-server.jsonc
  schema/
    main.sql
  migrations/
    _meta/
```

For PostgreSQL projects, `postgres-language-server.jsonc` is generated from the same init defaults so editor tooling points at the Declarative Schema entrypoint.

2. Configure your live database URL.

```bash
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
```

3. Edit the generated Declarative Schema entrypoint.

```sql
-- db/schema/main.sql
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
- `-d, --migrations-dir <PATH>`: migration output/read directory
- `-v, --verbose`: verbose output
- `-n, --no-color`: disable color output

Command-scoped options:

- `diff`, `generate`, `codegen`, and `queries` accept `--shadow-database-url <URL>` and `--pg-version <14|15|16|17|18>`.
- `create`, `generate`, `migrate`, `status`, and `down` accept migration options such as `--table <NAME>`, `--prefix <index|timestamp|unix>`, and `--generate-down` where applicable.
- `migrate` accepts `--dry` and optional mode subcommands: `all`, `steps <NUM>`, and `to <NAME>`.
- `codegen` accepts codegen options such as `--output <PATH>`, `--format <single|singlemodule|modules>`, and the tri-state derive toggles `--serde[=<bool>]` and `--sqlx[=<bool>]` (bare flag enables; `=false` disables).
- `queries` accepts `--sources <PATH>`, `--output <PATH>`, `--format <single|singlemodule|modules>`, `--models <PATH>`, and `--preview`.

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
| `codegen`                  | `code`     | Generate Rust, TypeScript, or Protobuf code from schema shape                                                              |
| `queries`                  | `q`        | Generate type-safe Rust query functions from annotated SQL files (PostgreSQL)                                              |
| `drop [migration]`         | -          | Remove a local migration, Down Migration, Snapshot, and Journal entry                                                      |

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

Use Custom Migrations for hand-written SQL — data backfills, operational SQL, or schema changes the Declarative Schema can't express. Any schema-shape changes they make are still tracked (see below), so the Declarative Schema and Snapshot chain stay in sync.

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

Custom Migrations are executable artifacts recorded in the Journal. Their SQL isn't final at creation time, so no Snapshot is written then — but the next time a diff is needed (`diff` or `generate`), Shki replays any not-yet-snapshotted migrations on a Shadow Database, introspects the result, and records a Snapshot for each. This keeps the Snapshot chain complete: if a Custom Migration changes the schema shape, that change is captured in the baseline so the next generated migration accounts for it (and won't re-emit DDL the Custom Migration already applied).

### Apply Migrations

```bash
shki migrate
```

`migrate` applies pending SQL files and records applied checksums in the live database migration table. It does not mutate local Snapshot files or the Journal. With no mode, `migrate` applies all pending migrations.

Apply a limited number of pending migrations:

```bash
shki migrate steps 2
```

Apply through a specific pending migration name:

```bash
shki migrate to 0003_add_users
```

Preview what would be applied without changing the database:

```bash
shki migrate --dry
shki migrate --dry steps 1
```

### Adopt An Existing Database

When a project already has a live database that predates `shki`, capture its shape
once as a baseline and commit the artifacts:

```bash
shki bootstrap            # introspect a dev/staging database, write the baseline
```

`bootstrap` only authors files — the initial migration, its Snapshot, the Directory
Schema, and the Journal entry. It never writes to the database. After this you no
longer need the original database; keep evolving the schema with `generate`/`create`.

Deploying to environments then depends on the target's state:

```bash
# Existing environment (already has the baseline schema):
shki adopt                # validate live shape == baseline, mark baseline applied, apply newer migrations

# Brand-new / empty environment:
shki migrate              # runs the baseline like any other migration, then the rest
```

`adopt` introspects the target, refuses if the live shape drifts from the committed
baseline Snapshot (override with `--force`), records the baseline as applied _without
executing_ it, and then applies any newer migrations. Use `--mark-only` to stop after
marking, `--dry-run` to preview, or pass a migration name to adopt up to a specific
point. `adopt` is idempotent — re-running it only applies what is still pending.

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

Codegen options:

| Option              | Purpose                                                                                                                                                                   |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `output`            | Default output path when `--output` is not provided. Relative paths resolve from `root`.                                                                                  |
| `format`            | Output layout: `single`, `singlemodule`, or `modules`.                                                                                                                    |
| `struct_derives`    | Replaces the default derives attached to generated structs.                                                                                                               |
| `struct_attributes` | Extra raw attributes added above generated structs.                                                                                                                       |
| `enum_derives`      | Replaces the default derives attached to generated enums.                                                                                                                 |
| `enum_attributes`   | Extra raw attributes added above generated enums.                                                                                                                         |
| `struct_renames`    | Exact table-name to generated struct-name overrides. These apply before `struct_pattern`.                                                                                 |
| `struct_pattern`    | Pattern for generated struct names. `{}` is replaced with the resolved base name. For table `users`, the base is `User`; pattern `{}Row` produces `UserRow`.              |
| `enum_renames`      | Exact enum-name to generated enum-name overrides. These apply before `enum_pattern`.                                                                                      |
| `enum_pattern`      | Pattern for generated enum names. `{}` is replaced with the resolved base name. For enum `user_status`, the base is `UserStatus`; pattern `Db{}` produces `DbUserStatus`. |
| `type_overrides`    | SQL type to generated type overrides. Built-in types use lowercase keys like `jsonb`; custom PostgreSQL types may use schema-qualified keys like `public.money`.          |
| `serde`             | Convenience toggle: injects `serde::Serialize`/`Deserialize` derives and `#[serde(rename)]` attributes. Defaults to `false`. Kept out of `struct_derives`/`enum_derives` so it can be toggled.                                                       |
| `sqlx`              | Convenience toggle: injects `sqlx::FromRow` (structs) / `sqlx::Type` (enums) derives and `#[sqlx(...)]` attributes. Defaults to `true`; set `false` for plain types with no sqlx coupling. Kept out of the derive lists so it can be toggled. |
| `include_tables`    | If non-empty, only listed table names are generated.                                                                                                                      |
| `exclude_tables`    | Listed table names are skipped. Applied after `include_tables`.                                                                                                           |
| `verbose`           | Prints generated code to stdout as well as writing files.                                                                                                                 |
| `impl_file_name`    | File name stem for hand-written impl files in `modules` mode.                                                                                                             |

Name resolution order is: explicit rename, default casing, then pattern. Struct defaults singularize table names and use PascalCase, so `users` becomes `User`. Enum defaults use PascalCase, so `user_status` becomes `UserStatus`.

### Generate Typed Queries (PostgreSQL)

`queries` turns annotated `*.sql` files into type-safe Rust functions backed by `sqlx`. Each query becomes a function with typed parameters and a typed result. Unlike `sqlx::query!`, the types are resolved at generation time by **describing** each query against the Shadow Database (the same embedded/external PostgreSQL used by `diff`/`generate`), so **no live production database is required at your compile time** and the generated code uses sqlx's runtime API — it is not re-checked against `DATABASE_URL`.

```bash
# Default: read <root>/queries, print to stdout
shki queries

# Read a file or directory, write to a file
shki queries --sources db/queries --output src/queries.rs

# Preview without writing
shki queries --sources db/queries --preview
```

Annotate each query with a name and cardinality, sqlc-style. The `name:` is the function name verbatim (normalized to `snake_case`); no prefix is added.

```sql
-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;

-- name: active_users :many
SELECT id, email FROM users WHERE active = true;

-- name: deactivate_user :exec
UPDATE users SET active = false WHERE id = $1;

-- name: user_by_email :one
SELECT * FROM users WHERE email = $email;
```

Cardinality controls the return shape:

| Tag      | Returns                  |
| -------- | ------------------------ |
| `:one`   | `Result<Option<Row>>`    |
| `:many`  | `Result<Vec<Row>>`       |
| `:exec`  | `Result<u64>` (rows affected) |
| `:batch` | A paginated `:many` — `Result<Page<Row>>` (limit/offset) |

Features:

- **Reuses schema types.** When a query's result columns map, in full and in order, to a known table, the function returns that table's generated struct (e.g. `Option<User>`) instead of a parallel row type; columns whose type is a known enum reuse the generated enum. Projections and joins get a synthesized per-query row struct named from the query (`active_users` → `ActiveUsersRow`). The generated module imports these types with a `use` path derived from your output layout (override with `models`).
- **Schema-driven nullability.** `RowDescription` does not report nullability, so it is inferred from the Declarative Schema: a column traced to a base-table column honors its `NOT NULL` constraint (`T` vs `Option<T>`); anything the schema cannot prove (expressions, function results, outer-join columns) defaults to `Option<T>`.
- **Named arguments.** A query may bind parameters as `$name` (e.g. `$email`) instead of positional `$1`, producing a self-documenting signature (`user_by_email(executor, email: String)`) rather than positional `arg1`. shki rewrites `$name` to `$n` before describing; the names exist only in the Rust signature. A single query must use one style or the other — mixing `$name` and `$1` is rejected.
- **Pagination (`:batch`).** Two explicit modes:
  - **Limit/offset** — a query carrying a `LIMIT $limit OFFSET $offset` placeholder takes a shared `Pagination { limit, offset }` by reference and returns `Result<Page<Row>>`. `Pagination`/`Page<T>` are emitted once and reused.
  - **Cursor/keyset** — selected by a `:keyset` modifier listing the cursor bind params (e.g. `-- name: events_after :batch :keyset $1 $2`). The function takes a `cursor: &CursorPagination<K>` (where `K` is the keyset type, a tuple for multiple keys). `CursorPagination<K>` is emitted once.

Limitations:

- **PostgreSQL only.** Describe-based typing relies on PostgreSQL; MySQL/SQLite query codegen is not implemented.
- **Rust/sqlx only.** TypeScript/Protobuf query output is not implemented (schema `codegen` covers those for types).
- **Keyset next-cursor is not derived.** Cursor `:batch` currently returns `Result<Vec<Row>>` and does not compute the *next* cursor from the last row; `CursorPagination`'s `next`/`prev` are caller-managed for now.
- **Generated query rows always derive `sqlx::FromRow`**, regardless of the `[codegen] sqlx` toggle, since they are decoded by sqlx.
- The Shadow Database is started for the describe step, so query codegen pays the same startup cost as `diff`/`generate`.

Configure in `[queries]`:

```toml
[queries]
sources = "db/queries"          # SQL file or directory (default: <root>/queries)
output = "src/db/queries.rs"     # output file; prints to stdout if omitted
format = "single"                # output layout, as in [codegen]
# models is optional — see below. By default it is derived from the
# codegen/queries output paths, e.g. with [codegen] output = "src/db/models.rs"
# the generated module imports `use super::models::*;`.
```

| Option    | Purpose                                                                                              |
| --------- | ---------------------------------------------------------------------------------------------------- |
| `sources` | SQL file or directory of annotated `*.sql` queries. Relative paths resolve from `root`. Default `<root>/queries`. |
| `output`  | Output file for generated Rust. Prints to stdout when omitted. Relative paths resolve from `root`.   |
| `format`  | Output layout: `single`, `singlemodule`, or `modules` (shared with `[codegen]`).                     |
| `models`  | Rust module path imported as `use <path>::*;` so generated functions can name your schema structs/enums. **Optional** — derived from the `[codegen]`/`[queries]` output paths when unset (sibling files share a directory, so e.g. `models.rs` + `queries.rs` → `super::models`). Set it (e.g. `crate::models`) only to override that for non-standard layouts; it must be a Rust module path, not a file path. |

The schema type mapping, naming/rename config, output modes, and `--preview` are shared with `[codegen]`; see [ADR 0001](docs/adr/0001-typed-query-codegen.md) for the full design.

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
- `bootstrap` to author a baseline from an existing database, and `adopt` to validate and adopt an existing environment against it.
- Code generation from current Declarative Schema or Snapshot JSON for Rust, TypeScript, and Protobuf.

Still in progress:

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

## Related Projects

`shki` stands on the shoulders of projects that explored these ideas first:

- [pgschema](https://github.com/pgschema/pgschema) — declarative, Terraform-style schema management for PostgreSQL.
- [jayy-lmao/sql-gen](https://github.com/jayy-lmao/sql-gen) — generating typed Rust code from a live database schema.
- [squirrel](https://github.com/giacomocavalieri/squirrel) — type-safe SQL code generation from introspected queries.
- [goose](https://github.com/pressly/goose) - A database migration tool. Supports SQL migrations and Go functions.
