---
title: shki diff
description: Preview the Migration Plan without writing anything.
slug: 0.10.10/commands/diff
---

```bash
shki diff [--shadow-database-url URL] [--pg-version 14..18]
```

Compiles the Declarative Schema in the Shadow Database, snapshots the result,
and compares it with the latest Snapshot recorded in the Journal. Prints an
object-level summary of the resulting Migration Plan, including rename
candidates.

`diff` writes no files, prints no SQL, and never touches the live database. It
is the safe command to run as often as you like.

## Options

| Flag                          | Purpose                                                                  |
| ----------------------------- | ------------------------------------------------------------------------ |
| `--shadow-database-url <URL>` | Use an external Shadow Database. Env: `SHKI_SHADOW_DATABASE_URL`.        |
| `--pg-version <14…18>`        | Embedded PostgreSQL major version. Default `18`. Env: `SHKI_PG_VERSION`. |
| `-v, --verbose`               | More detail about the compile and diff.                                  |

## Examples

```bash
shki diff
shki diff --pg-version 16
shki diff --shadow-database-url postgres://user:pass@localhost:5432/shki_shadow
```

## Notes

* If any Custom Migrations have no Snapshot yet, `diff` replays them on the
  Shadow Database first so the baseline is complete.
* "No schema changes detected" means the Declarative Schema and the latest
  Snapshot agree — nothing to generate.
* An index declared `CREATE INDEX CONCURRENTLY` diffs exactly like a plain one:
  the keyword is a creation strategy, not schema state. It only changes what
  [`shki generate`](../../commands/generate/#concurrent-indexes) writes.
* PostgreSQL only: Declarative Schema compilation is not implemented for MySQL,
  and is planned for SQLite.

See also: [`shki generate`](../../commands/generate/),
[How It Works](../../getting-started/how-it-works/).
