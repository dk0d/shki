---
title: shki migrate
description: Apply pending migrations to the live database.
---

```bash
shki migrate [--dry] [all | steps <N> | to <NAME>]
```

Alias: `shki up`. Applies pending migration files in order and records each one,
with its checksum, in the migrations table (`shki.__shki_migrations` by
default). Local Snapshots and the Journal are never modified — generation and
application are separate concerns.

Each migration runs inside a transaction unless it carries the
`shki:no-transaction` directive.

## Modes

| Mode              | Behavior                                                     |
| ----------------- | -------------------------------------------------------------- |
| *(none)* / `all`  | Apply every pending migration.                                |
| `steps <N>`       | Apply the next `N` pending migrations.                        |
| `to <NAME>`       | Apply through the named pending migration, inclusive.         |

## Options

| Flag                                | Purpose                                        |
| ----------------------------------- | ------------------------------------------------ |
| `--dry`                             | Show what would be applied; change nothing.     |
| `-T, --table <NAME>`                | Migrations table name.                          |
| `-u, --database-url <URL>`          | Target database. Env: `DATABASE_URL`.           |
| `--prefix <index\|timestamp\|unix>` | File name prefix style.                         |

## Examples

```bash
shki migrate                    # everything pending
shki migrate --dry              # preview first
shki migrate steps 2            # next two only
shki migrate to 0003_add_users  # through a specific migration
```

## Notes

- Requires a database URL; nothing else about the project has to be present
  beyond the migrations directory.
- A checksum mismatch on an already-applied migration is reported rather than
  ignored — see [Troubleshooting](/shki/reference/troubleshooting/#checksum-mismatch-on-an-applied-migration).
- On a brand-new environment, `migrate` runs the baseline migration like any
  other. On an environment that already has the baseline schema, use
  [`shki adopt`](/shki/commands/adopt/) instead.

See also: [`shki status`](/shki/commands/status/),
[`shki down`](/shki/commands/down/).
