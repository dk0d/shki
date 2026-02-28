<dp align="center">
<img src="/assets/shki-logo.png" alt="shki-logo" style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</dp>

> [!WARNING]
> shki is still a work in progress. PostgreSQL is the only fully implemented dialect right now.

# shki

Declarative database schema management and migrations with Lua + Rust.

Define your target schema in Lua, diff against the live database, and let shki generate migration SQL.

## Current status

- PostgreSQL: supported for schema diffing, migration generation, migrate/down, introspection, bootstrap, squash, and codegen.
- MySQL and SQLite: CLI flags and structure exist, but core introspection/diff flow is not complete yet.
- Schema language: Lua.
- Code generation: Rust, TypeScript, and Protobuf outputs.

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

```bash
# 1) Initialize a project
shki init db

# 2) Configure DB URL (or put it in shki.toml/.env)
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'

# 3) Edit db/init.lua, then preview diff
shki --config db/shki.toml diff

# 4) Generate and apply migration
shki --config db/shki.toml generate add_users
shki --config db/shki.toml migrate
```

`shki init db` creates:

```text
db/
├── migrations/
│   └── _meta/
├── lua/
├── .luacats/
├── .luarc.json
├── selene.toml
├── shki.yml
├── init.lua
└── shki.toml
```

## Commands

All commands support global options:

- `-c, --config <PATH>` (default `shki.toml`)
- `-l, --dialect <pg|postgres|postgresql|mysql|sqlite>`
- `-u, --database-url <URL>` (env fallback: `DATABASE_URL`)
- `-o, --out <PATH>`
- `-v, --verbose`

| Command | Alias | Purpose |
| - | - | - |
| `init [path]` | `i` | Create project files (`--simple` for config-only) |
| `generate [name]` | `gen` | Diff schema vs DB and write migration (`--dry-run` prints SQL) |
| `migrate` | `up` | Apply pending migrations (`--dry-run` supported) |
| `down [count]` | - | Roll back migrations using `.down.sql` files |
| `create <name>` | `new` | Create blank SQL migration (`--with-down`, `--sql`, `--sql-file`) |
| `drop [migration]` | - | Delete a migration file |
| `status` | `s` | Show applied/pending migration status |
| `diff` | - | Show schema diff (`--sql` for SQL output) |
| `pull` | - | Introspect DB to `sql`, `json`, or `rust` |
| `bootstrap [name]` | `strap` | Create baseline migration from existing DB |
| `squash` | `sq` | Collapse existing migration history into one baseline |
| `codegen <language>` | `code` | Generate `rust`, `typescript`, or `protobuf` models |

## Common workflows

### Schema-first (recommended)

```bash
shki diff
shki generate add_posts
shki migrate
```

### SQL-first migration runner

```bash
shki init --simple
shki create add_users_table --with-down
shki migrate
```

### Adopt an existing database (Experimental)

```bash
# baseline migration from current DB state
shki bootstrap initial_baseline

# optional: also write Lua schema from introspection
shki bootstrap initial_baseline --write-lua
```

### Squash migration history (Experimental)
 
```bash
shki squash --name baseline_after_v1
```

### Introspect schema

```bash
shki pull --format sql
shki pull --format json --output schema.json
```

## Code generation

```bash
# Rust
shki codegen --out src/models.rs --mode single rust

# TypeScript (flavor: type | interface)
shki codegen --out src/models.ts --mode single typescript interface

# Protobuf
shki codegen --out proto --mode modules protobuf
```

Available `--mode` values: `single`, `single-module`, `modules`.

## Configuration

Set config in `shki.toml`, env vars, or CLI flags.

```toml
root = "."
dialect = "postgres"
schema = "init.lua"
out = "migrations"
database_url = "postgres://user:pass@localhost:5432/mydb"
breakpoints = true

[migrations]
table = "__shki_migrations"
prefix = "timestamp" # index | timestamp | unix
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

## Note on down migrations

Auto-generated down migrations are best for local iteration. For production, prefer forward-only migration strategy.

Known hard cases that may need manual down SQL:

- enum value additions
- sequence alterations
- complex column alterations

## Roadmap

- [ ] Complete MySQL support
- [ ] Complete SQLite support
- [ ] Rust-native schema definitions
- [ ] Extended codegen customization (ecosystem-specific derives)
- [ ] Schema linting/validation

## Contributing

Issues and pull requests are welcome.

## License

MIT, see [LICENSE](LICENSE).
