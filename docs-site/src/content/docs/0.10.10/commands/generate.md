---
title: shki generate
description: Write migration artifacts from the Declarative Schema.
slug: 0.10.10/commands/generate
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

## Concurrent indexes

Declare an index `CREATE INDEX CONCURRENTLY` in the Declarative Schema to get
the online rollout generated for you. `generate` prompts for confirmation, then
splits the plan in two migrations:

```text
migrations/<prefix>_<name>.sql           # every other change, one transaction
migrations/<prefix>_<name>-indexes.sql   # -- shki:no-transaction
```

For example, adding a column and a concurrent index to the schema:

```sql
-- schema/main.sql — declare the desired shape, no ALTERs needed
CREATE TABLE scan (
  id bigint PRIMARY KEY,
  payload text NOT NULL,
  captured_at timestamptz NOT NULL
);
CREATE INDEX CONCURRENTLY scan_captured_at_idx ON scan (captured_at);
```

```bash
$ shki generate capture-at
? 1 index(es) are declared CONCURRENTLY and will be written as a separate
  no-transaction migration (CREATE INDEX CONCURRENTLY IF NOT EXISTS). Continue? (Y/n)
```

```sql
-- 0007_capture-at.sql
ALTER TABLE "scan" ADD COLUMN "captured_at" TIMESTAMPTZ NOT NULL;
```

```sql
-- 0008_capture-at-indexes.sql
-- shki:no-transaction

CREATE INDEX CONCURRENTLY IF NOT EXISTS "scan_captured_at_idx" ON "scan" ("captured_at");
```

The second file builds each declared index with
`CREATE INDEX CONCURRENTLY IF NOT EXISTS`, one `--> +statement` segment per
index, so nothing takes a write-blocking lock on a live table. Its down
migration is the matching no-transaction `DROP INDEX CONCURRENTLY IF EXISTS`.
The idempotency guard exists only here, where a partial failure replays the
file from the top — everything else shki generates is strict DDL, so a name
collision fails loudly instead of silently keeping an index whose definition
may not match. Declining the prompt fails the whole generation and writes
nothing; non-interactive runs fail the same way. If the only change is
concurrent indexes, just the one no-transaction migration is written. Each
migration gets its own Snapshot: the first records the intermediate state
(everything except the not-yet-built indexes), the second records the full
desired state, keeping the Snapshot chain complete.

`CONCURRENTLY` is a creation strategy, not schema state — it isn't recorded in
the Snapshot, and adding or removing the keyword on an already-existing index
diffs as no change. See
[Declarative Schema](../../guides/declarative-schema/#concurrent-index-builds).

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
