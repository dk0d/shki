---
title: shki init
description: Scaffold a new shki project.
slug: 0.10.7/commands/init
---

```bash
shki init [PATH] [--dialect postgres|mysql|sqlite]
```

Alias: `shki i`. `PATH` defaults to the current directory; the dialect defaults
to `postgres`.

## What it writes

```text
<path>/
  shki.toml                       # dialect, schema/migrations paths, migrations table
  schema/main.sql                 # Declarative Schema entrypoint (commented example)
  migrations/_meta/               # Snapshot and Journal metadata
  postgres-language-server.jsonc  # PostgreSQL only: editor tooling config
```

`shki.toml` is always written. `schema/main.sql` and
`postgres-language-server.jsonc` are only written when absent, so re-running
`init` in an existing project won't clobber your schema.

The generated config is deliberately small:

```toml
dialect = "postgres"
schema = "schema"
migrations_dir = "migrations"
timeout_seconds = 2

[migrations]
schema = "shki"
table = "__shki_migrations"
prefix = "index"
generate_down = false
```

## Examples

```bash
shki init                          # scaffold in place, PostgreSQL
shki init db                       # scaffold into ./db
shki init db --dialect sqlite      # migration-runner project, no Declarative Schema
```

## Next

Set `database_url` in `shki.toml` or export `DATABASE_URL`, describe your
schema in `schema/main.sql`, then run [`shki diff`](../../commands/diff/).

See also: [Quick Start](../../getting-started/quick-start/),
[Configuration](../../reference/configuration/).
