---
title: shki create
description: Create a Custom Migration for hand-written SQL.
---

```bash
shki create <NAME> [--with-down] [--sql SQL | --sql-file PATH] [--edit]
```

Alias: `shki new`. Equivalent to `shki generate <NAME> --custom`.

Creates an empty (or seeded) migration file for SQL that the Declarative Schema
can't express: data backfills, operational statements, and so on. No Shadow
Database is started and no Snapshot is written — the SQL isn't final yet.

For `CREATE INDEX CONCURRENTLY` you usually don't need a Custom Migration
anymore: declare the index `CONCURRENTLY` in the Declarative Schema and
[`shki generate`](../../commands/generate/#concurrent-indexes) writes the
no-transaction rollout for you.

## Options

| Flag                                | Purpose                             |
| ----------------------------------- | ----------------------------------- |
| `--with-down`                       | Also create a `.down.sql` file.     |
| `--sql <SQL>`                       | Seed the migration with inline SQL. |
| `--sql-file <PATH>`                 | Seed the migration from a file.     |
| `-e, --edit`                        | Open the created file in `$EDITOR`. |
| `--prefix <index\|timestamp\|unix>` | File name prefix style.             |
| `-T, --table <NAME>`                | Migrations table name.              |

## Examples

```bash
shki create backfill_user_emails --with-down
shki create add_users_index --sql 'CREATE INDEX idx_users_email ON users(email);'
shki create add_audit_table --sql-file ./sql/add_audit_table.sql
shki create tweak_defaults --edit
```

## How it stays in sync

Custom Migrations are recorded in the Journal in order, but without a Snapshot.
The next time a diff is needed (`diff` or `generate`), shki replays every
not-yet-snapshotted migration on the Shadow Database, introspects the result,
and records a Snapshot for each. A schema change you made by hand therefore
becomes part of the baseline, and the next generated migration won't re-emit DDL
that already ran.

## Statements that can't run in a transaction

Each migration runs in one transaction. Opt out with a directive when that's
impossible:

```sql
-- shki:no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_scan_captured_at ON scan (captured_at);
--> +statement
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_scan_profile ON scan (profile_id);
```

Such migrations must be idempotent — see
[running outside a transaction](../../guides/migrations/#running-outside-a-transaction).

See also: [`shki generate`](../../commands/generate/),
[`shki migrate`](../../commands/migrate/).
