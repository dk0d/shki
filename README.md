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

By default, `shki` uses managed embedded PostgreSQL. You can pin the embedded PostgreSQL major version:

```bash
shki generate create_users --shadow-database-postgres-version 16
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
- `--shadow-database-url <URL>`: external Shadow Database URL
- `--shadow-database-postgres-version <14|15|16|17|18>`: embedded PostgreSQL major version
- `--migrations-dir <PATH>`: migration output/read directory
- `-v, --verbose`: verbose output
- `--table <NAME>`: migrations table name
- `--schema <SCHEMA>`: migrations table schema for PostgreSQL
- `--prefix <index|timestamp|unix>`: migration file name prefix style
- `--generate-down`: generate Down Migrations by default

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
| `codegen`                  | `code` | Generate Rust, TypeScript, or Protobuf code from a Snapshot  |

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

### Generate Code From A Snapshot

```bash
shki codegen rust --schema migrations/_meta/0000_create_users.snapshot.json --out src/schema
shki codegen typescript --schema migrations/_meta/0000_create_users.snapshot.json --out src/schema
shki codegen protobuf --schema migrations/_meta/0000_create_users.snapshot.json --out proto
```

## Configuration

Set config in `shki.toml`, environment variables, or CLI flags.

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
shadow_database_postgres_version = 16

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
SHKI_SHADOW_DATABASE_POSTGRES_VERSION=16
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
- Migration runner, status, checksum validation, and Down Migrations.
- Code generation from Snapshot JSON for Rust, TypeScript, and Protobuf.

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
