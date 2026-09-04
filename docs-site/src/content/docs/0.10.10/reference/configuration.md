---
title: Configuration
description: shki.toml keys, environment variables, precedence, and path resolution.
slug: 0.10.10/reference/configuration
---

Configuration comes from three layers, each overriding the one before it:

1. `shki.toml` (or `--config <PATH>`)
2. environment variables (and `.env` in the working directory)
3. CLI flags

`shki config` prints the merged result — use it when a value isn't what you
expect.

## Paths

If `root` is omitted, relative paths resolve from the directory containing the
config file. If `root` is set, relative paths such as `schema`, `out`, dump
outputs, and codegen outputs resolve from `root`.

## `shki.toml`

`shki init` writes a starting `shki.toml`; everything below is optional beyond
`dialect` and a database URL.

```toml
root = "db"
dialect = "postgres"
database_url = "postgres://user:pass@localhost:5432/mydb"
schema = "schema"            # Declarative Schema file or directory entrypoint
migrations_dir = "migrations" # also accepted as `out`
timeout_seconds = 2          # database connection timeout
breakpoints = true           # insert statement breakpoints in generated SQL

# Optional. If omitted, shki uses managed embedded PostgreSQL. An external
# database must be dedicated to shki and marked before use:
# COMMENT ON DATABASE shki_shadow IS 'shki:shadow';
shadow_database_url = "postgres://user:pass@localhost:5432/shki_shadow"

# Optional. Supported: 14, 15, 16, 17, 18.
pg_version = 16

[migrations]
table = "__shki_migrations"
schema = "shki"            # schema holding the migrations table (PostgreSQL)
prefix = "index"           # index | timestamp | unix
generate_down = false
```

| Key                        | Default             | Purpose                                                    |
| -------------------------- | ------------------- | ---------------------------------------------------------- |
| `root`                     | config file's dir   | Base for relative paths.                                   |
| `dialect`                  | –                   | `postgres`, `mysql`, or `sqlite`.                          |
| `database_url`             | `$DATABASE_URL`     | Live database connection URL.                              |
| `schema`                   | `schema`            | Declarative Schema file or directory entrypoint.           |
| `migrations_dir`           | `migrations`        | Migration output/read directory. Alias: `out`.             |
| `shadow_database_url`      | embedded PostgreSQL | External Shadow Database. Must differ from `database_url`. |
| `pg_version`               | `18`                | Embedded PostgreSQL major version: 14–18.                  |
| `timeout_seconds`          | `2`                 | Database connection timeout.                               |
| `breakpoints`              | `true`              | Emit statement breakpoints in generated migration SQL.     |
| `migrations.table`         | `__shki_migrations` | Table recording applied migrations.                        |
| `migrations.schema`        | `shki`              | Schema holding that table (PostgreSQL).                    |
| `migrations.prefix`        | `index`             | File name prefix style: `index`, `timestamp`, or `unix`.   |
| `migrations.generate_down` | `false`             | Write Down Migrations alongside up migrations.             |

MySQL and SQLite remain supported for migration-runner workflows:

```toml
# MySQL
dialect = "mysql"
database_url = "mysql://user:pass@localhost:3306/mydb"

# SQLite
dialect = "sqlite"
database_url = "sqlite://db/app.db"
```

See [Code Generation](../../guides/codegen/) for `[codegen]` and
[Typed Queries](../../guides/queries/) for `[queries]`.

## Environment variables

Bare names are read directly; anything else uses the `SHKI_` prefix, with `__`
separating nested table keys.

```bash
DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
SHKI_SHADOW_DATABASE_URL='postgres://user:pass@localhost:5432/shki_shadow'
SHKI_PG_VERSION=16
SHKI_MIGRATIONS__TABLE='__shki_migrations'
SHKI_MIGRATIONS__PREFIX='timestamp'
SHKI_MIGRATIONS__GENERATE_DOWN=true
```

`shki` also reads `.env` from the current working directory, which is the usual
way to keep `DATABASE_URL` out of the config file.
