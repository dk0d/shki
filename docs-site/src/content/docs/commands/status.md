---
title: shki status
description: List migrations, applied state, and checksum problems.
---

```bash
shki status
```

Alias: `shki s`. Prints the target database (with credentials masked), then a
table of every local migration and whether it has been applied.

When a database connection is available, `status` also validates the checksums
of applied migrations and reports any whose file has changed since it ran. This
is the command to run before `migrate` in an unfamiliar environment.

## Options

| Flag                       | Purpose                               |
| -------------------------- | ------------------------------------- |
| `-T, --table <NAME>`       | Migrations table name.                |
| `-u, --database-url <URL>` | Target database. Env: `DATABASE_URL`. |
| `-v, --verbose`            | More detail per migration.            |

## Examples

```bash
shki status
shki status -u postgres://user:pass@prod-host:5432/mydb
```

## Notes

- Without a reachable database, `status` still lists local migrations; it just
  can't say which are applied or validate checksums.
- A checksum failure means the file changed after it was applied. Restore the
  original SQL and write a follow-up migration instead of editing history — see
  [Troubleshooting](../../reference/troubleshooting/#checksum-mismatch-on-an-applied-migration).

See also: [`shki migrate`](../../commands/migrate/),
[`shki config`](../../commands/config/).
