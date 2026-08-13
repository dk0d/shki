---
title: shki down
description: Roll back applied migrations using their Down Migrations.
---

```bash
shki down [COUNT] [--dry-run]
```

Rolls back the most recently applied migrations by running their `.down.sql`
files, newest first. `COUNT` defaults to `1`.

`down` lists what it is about to roll back and asks for confirmation before
touching anything. Only migrations that actually have a Down Migration file are
candidates; if none of the applied migrations have one, it says so and exits.

## Options

| Flag                       | Purpose                                         |
| -------------------------- | ----------------------------------------------- |
| `--dry-run`                | List what would be rolled back; change nothing. |
| `-T, --table <NAME>`       | Migrations table name.                          |
| `-u, --database-url <URL>` | Target database. Env: `DATABASE_URL`.           |

## Examples

```bash
shki down --dry-run    # see what would happen
shki down              # roll back the newest applied migration
shki down 3            # roll back the newest three
```

## Notes

- Down Migrations are for local iteration. Rolling back in production is a data
  loss risk that a `DROP` in a down file will happily deliver; prefer a
  forward-fix migration there.
- Generate Down Migrations with `shki generate <name> --down`,
  `shki create <name> --with-down`, or `[migrations] generate_down = true`.
- Requires a database URL.

See also: [`shki migrate`](/shki/commands/migrate/),
[`shki drop`](/shki/commands/drop/) for removing a migration that was never
applied.
