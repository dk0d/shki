---
title: shki generate
description: Write migration artifacts from the Declarative Schema.
slug: 0.10.8/commands/generate
---

```bash
shki generate <NAME> [--down] [--custom]
```

Alias: `shki gen`. Runs the same compile-and-diff as
[`shki diff`](../../commands/diff/), then renders the Migration Plan to disk.

## What it writes

```text
migrations/<prefix>_<name>.sql               # the migration
migrations/<prefix>_<name>.down.sql          # with --down
migrations/_meta/<prefix>_<name>.snapshot.json
migrations/_meta/_journal.json               # updated
```

It prints the paths it wrote followed by a preview of the diff. Nothing is
applied to the live database — that's [`shki migrate`](../../commands/migrate/).

## Options

| Flag                                | Purpose                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------ |
| `--down`                            | Also write a Down Migration.                                                         |
| `--custom`                          | Create a Custom Migration instead (same as [`shki create`](../../commands/create/)). |
| `-T, --table <NAME>`                | Migrations table name.                                                               |
| `--prefix <index\|timestamp\|unix>` | File name prefix style. Default `index`.                                             |
| `--generate-down`                   | Config-equivalent of `--down`.                                                       |
| `--shadow-database-url <URL>`       | External Shadow Database.                                                            |
| `--pg-version <14…18>`              | Embedded PostgreSQL major version.                                                   |

Set `[migrations] generate_down = true` in `shki.toml` to make Down Migrations
the default.

## Renames

When the diff finds objects that look renamed rather than dropped and recreated,
`generate` prompts before rendering. Accepting a rename replaces the
drop/create pair with a rename statement where the dialect supports one — which
preserves the data a drop would have destroyed. Renaming a table can surface
follow-up prompts for the objects that depend on it.

## Examples

```bash
shki generate add_users_table
shki generate add_users_table --down
shki generate backfill_emails --custom       # hand-written SQL instead
shki generate add_index --prefix timestamp
```

## Notes

* "No schema changes detected" means there is nothing to generate.
* Commit the migration, its Snapshot, and the updated Journal together — the
  Snapshot is the baseline the next `generate` diffs against.

See also: [Migrations guide](../../guides/migrations/),
[`shki drop`](../../commands/drop/) to undo a generated migration locally.
