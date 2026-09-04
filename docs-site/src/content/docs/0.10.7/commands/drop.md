---
title: shki drop
description: Remove a local migration and its artifacts.
slug: 0.10.7/commands/drop
---

```bash
shki drop [MIGRATION] [--force]
```

Deletes a local migration's up file, Down Migration, Snapshot, and Journal
entry. This is local cleanup for a migration you generated and don't want —
it does not change the database.

Called without `MIGRATION`, it opens a fuzzy picker over the local migrations
(applied ones are labeled). With a name, it matches exactly or by suffix, so
`shki drop add_users` finds `0003_add_users`.

## Applied migrations are refused

If the selected migration is recorded as applied in the target database, `drop`
errors out:

```text
Cannot drop applied migration '0003_add_users'. Roll it back before dropping it.
```

Roll it back with [`shki down`](../../commands/down/) first, or pass `--force` to
skip the database check entirely (nothing can be detected as applied, so nothing
blocks the delete).

## Options

| Flag                       | Purpose                                                   |
| -------------------------- | --------------------------------------------------------- |
| `-f, --force`              | Skip the database check for applied state.                |
| `-u, --database-url <URL>` | Target database used for that check. Env: `DATABASE_URL`. |

## Examples

```bash
shki drop                    # pick from a list
shki drop 0003_add_users     # by full name
shki drop add_users          # by suffix
shki drop 0003_add_users -f  # offline, no database check
```

## Notes

Dropping the newest generated migration also drops the Snapshot that the next
`generate` would have used as its baseline, which is what makes it safe to
regenerate: the following `diff`/`generate` compares against the previous
Snapshot instead.

See also: [`shki generate`](../../commands/generate/).
