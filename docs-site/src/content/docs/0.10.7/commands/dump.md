---
title: shki dump
description: Export a live database shape as SQL, JSON, or a Directory Schema.
slug: 0.10.7/commands/dump
---

```bash
shki dump [--format sql|json] [--dirs] [--output PATH] [--schema NAME] [--force]
```

Introspects the live database and renders its current shape. Reads only — it
never modifies the database, and it does not touch migrations or the Journal.

## Output modes

| Invocation                                | Result                                                                               |
| ----------------------------------------- | ------------------------------------------------------------------------------------ |
| `shki dump`                               | SQL to stdout.                                                                       |
| `shki dump --format json --output f.json` | A JSON Snapshot, the same format shki records internally.                            |
| `shki dump --dirs --output schema`        | A Directory Schema: `main.sql`, `extensions/`, and schema-scoped object directories. |
| `shki dump --dirs`                        | Preview of that directory layout, written nowhere.                                   |

## Options

| Flag                       | Purpose                                               |
| -------------------------- | ----------------------------------------------------- |
| `-f, --format <json\|sql>` | Output format. Default `sql`.                         |
| `--output <PATH>`          | Output file or directory. Defaults to stdout/preview. |
| `--dirs`                   | Emit a Directory Schema instead of a single file.     |
| `--force`                  | Overwrite file collisions in directory mode.          |
| `--schema <NAME>`          | Schema to dump (PostgreSQL, default `public`).        |
| `-u, --database-url <URL>` | Database to introspect. Env: `DATABASE_URL`.          |

## Examples

```bash
shki dump                                        # eyeball the live shape
shki dump --format json --output snapshot.json   # capture a Snapshot
shki dump --dirs --output schema                 # start a Declarative Schema from it
shki dump --schema billing                       # a non-public schema
```

## Notes

* Works on PostgreSQL, MySQL, and SQLite; PostgreSQL has the richest catalog
  coverage.
* To adopt an existing database, prefer [`shki bootstrap`](../../commands/bootstrap/),
  which writes the Directory Schema *and* the baseline migration, Snapshot, and
  Journal entry in one step.

See also: [Declarative Schema](../../guides/declarative-schema/).
