---
title: shki bootstrap
description: Author a baseline migration from an existing database.
---

```bash
shki bootstrap [NAME] [--dry-run] [--force] [--schema NAME]
```

Alias: `shki strap`. For projects whose database predates shki. It introspects a
live database (use dev or staging) and writes the artifacts that describe its
current shape as the starting point of history: the initial migration, its
Snapshot, a Directory Schema, and the Journal entry. `NAME` defaults to
`bootstrap`.

:::note
`bootstrap` only authors files. It never writes to the database — not even the
migrations table.
:::

## Options

| Flag                       | Purpose                                                      |
| -------------------------- | ------------------------------------------------------------ |
| `--dry-run`                | Show what would be generated without writing files.          |
| `--force`                  | Run even when migrations or Snapshots already exist locally. |
| `--schema <NAME>`          | Schema to bootstrap (PostgreSQL, default `public`).          |
| `-u, --database-url <URL>` | Database to introspect. Env: `DATABASE_URL`.                 |

## Examples

```bash
shki bootstrap --dry-run                 # inspect the plan first
shki bootstrap                           # write the baseline
shki bootstrap initial --schema billing  # name it, non-default schema
```

## After bootstrapping

Commit everything it wrote. From then on the original database is no longer
special — evolve the schema with [`shki generate`](../../commands/generate/) and
[`shki create`](../../commands/create/) as normal.

Deployment depends on the target environment's state:

```bash
shki adopt      # environment already has the baseline schema
shki migrate    # brand-new/empty environment: runs the baseline like any migration
```

See also: [`shki adopt`](../../commands/adopt/),
[Adopt an existing database](../../guides/migrations/#adopt-an-existing-database).
