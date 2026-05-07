<div align="center">
<img src="/assets/shki-logo.png" alt="shki-logo" style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</div>

> [!WARNING]
> `shki` is still a work in progress. Planned features may not be fully finished or entirely missing.

# shki

Database migrations with a smaller, rebuilt core.

`shki` is currently focused on the basics: initializing a project, creating SQL migration files, applying them, checking status, and rolling them back when down migrations exist.

Dialects Supported:

- [x] PostgreSQL
- [x] SQLite
- [ ] MySQL

## Back to basics

This rebuild intentionally steps back from the older, broader feature set for now.

The goal is to get the migration runner, configuration model, and command surface back into a clean and dependable state before re-introducing schema diffing, introspection, code generation, and the rest of the original vision.

That means the current README reflects what is available today, and the missing pieces are called out explicitly below instead of being implied.

## Current feature set

- Manual SQL migration workflow.
- Project initialization with `sql` mode and an early `lua` mode scaffold.
- Create migration files with optional inline SQL, SQL loaded from a file, and optional `.down.sql` companions.
- Apply pending migrations in order.
- Track applied migrations in a dedicated migrations table.
- Validate stored checksums against applied migration files.
- Show migration status.
- Roll back applied migrations with `down` files.
- Configure behavior through `shki.toml`, environment variables, and CLI flags.
- PostgreSQL is the primary supported backend in the current rebuild.

## Not back yet

These features existed previously or are still present as partial code paths, but they are not part of the current supported surface area:

- [ ] Schema diffing against a live database
- [ ] Generated migrations from Lua schema changes
- [ ] Full Lua schema authoring workflow and starter files
- [ ] Database introspection / `pull`
- [ ] Bootstrap / adopt-existing-database flow
- [ ] Squash existing migration history
- [ ] Code generation for Rust, TypeScript, or Protobuf
- [ ] Drop/delete migration helper commands
- [ ] MySQL support beyond basic dialect plumbing
- [ ] SQLite support beyond basic dialect plumbing
- [ ] Rust-native schema definitions
- [ ] Schema linting and validation passes

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

## Quick start

1. Initialize a project.

```bash
shki init db
```

By default this initializes the current SQL-first workflow. The generated `shki.toml` lives in the current directory and points `root` at `db`.

2. Configure your database URL in the shell, `.env`, or `shki.toml`.

```bash
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
```

3. Create a migration.

```bash
shki create add_users_table --with-down
```

4. Edit the generated SQL files in `db/migrations/`.

5. Apply pending migrations.

```bash
shki migrate
```

6. Check status or preview a rollback.

```bash
shki status
shki down --dry-run
```

## Commands

All commands support global options:

- `-c, --config <PATH>` (default `shki.toml`)
- `-l, --dialect <postgres|mysql|sqlite>`
- `-u, --database-url <URL>` (env fallback: `DATABASE_URL`)
- `-o, --out <PATH>`
- `-v, --verbose`
- `--table <NAME>`
- `--schema <SCHEMA>`
- `--prefix <index|timestamp|unix>`
- `--generate-down`

| Command         | Alias  | Purpose                                                      |
| --------------- | ------ | ------------------------------------------------------------ |
| `config`        | `conf` | Print the effective configuration                            |
| `init [path]`   | `i`    | Initialize a project directory                               |
| `migrate`       | `up`   | Apply pending migrations                                     |
| `create <name>` | `new`  | Create a blank migration for manual SQL editing              |
| `status`        | `s`    | Show migration status and checksum issues                    |
| `down [count]`  | -      | Roll back applied migrations with matching `.down.sql` files |

## Common workflows

### SQL-first migration runner

```bash
shki init db
shki create add_posts_table --with-down
shki migrate
```

### Seed a migration with SQL immediately

```bash
shki create add_users_index \
  --sql 'CREATE INDEX idx_users_email ON users(email);'
```

### Seed a migration from a file

```bash
shki create add_audit_table --sql-file ./sql/add_audit_table.sql
```

### Roll back one or more applied migrations

```bash
shki down --dry-run
shki down 1
shki down 3
```

## Configuration

Set config in `shki.toml`, env vars, or CLI flags.

Default config:

```toml
root = "db"
database_url = "postgres://user:pass@localhost:5432/mydb"
dialect = "postgres"
schema = "init.lua"
out = "./migrations"
breakpoints = true
verbose = false
timeout_seconds = 2
mode = "sql"

[migrations]
table = "__shki_migrations"
prefix = "index"
generate_down = false
```

Environment variables:

- `DATABASE_URL` is supported directly by the CLI.
- `SHKI_`-prefixed config vars are supported (use `__` for nesting), for example:

```bash
SHKI_DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
SHKI_MIGRATIONS__TABLE='__shki_migrations'
SHKI_MIGRATIONS__PREFIX='timestamp'
SHKI_MIGRATIONS__GENERATE_DOWN=true
```

shki also reads `.env` from the current working directory.

## Notes

### Down migrations

Down migrations are optional. `shki down` only considers applied migrations that have a matching `.down.sql` file.

### Checksum validation

When `status` or `migrate` can connect to the database, `shki` validates stored checksums for applied migrations and reports if a migration file has changed since it was applied.

### Lua mode

`init --mode lua` still exists, but the full schema-first Lua workflow is being brought back later as part of the broader rebuild.

## Contributing

Issues and pull requests are welcome.

## License

MIT, see [LICENSE](LICENSE).
