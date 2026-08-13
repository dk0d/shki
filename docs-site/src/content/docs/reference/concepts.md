---
title: Concepts And Scope
description: Snapshots, the Journal, checksum validation, dialect support, and what is still in progress.
---

## Snapshots and Journal

Snapshots record database shape. The Journal relates migrations to Snapshots and
is the history index for generated artifacts.

Schema-derived generation uses the latest schema Snapshot from the Journal as the
baseline. Custom Migrations can appear between schema-derived migrations without
breaking that chain.

## Checksum validation

When `status` or `migrate` can connect to the database, `shki` validates stored
checksums for applied migrations and reports if a migration file has changed
since it was applied.

## Supported dialects

| Workflow                                 | PostgreSQL | MySQL | SQLite  |
| ---------------------------------------- | ---------- | ----- | ------- |
| Apply/status/down migration runner       | yes        | yes   | yes     |
| Custom Migration creation                | yes        | yes   | yes     |
| Dump live shape as SQL/JSON              | yes        | yes   | yes     |
| Dump live shape as Directory Schema      | yes        | yes   | yes     |
| Declarative Schema compile/diff/generate | yes        | no    | planned |
| Rich Catalog introspection coverage      | strongest  | basic | basic   |

Declarative Schema generation is PostgreSQL-focused for now. The migration runner
remains dialect-aware for PostgreSQL, MySQL, and SQLite.

## Project scope

Implemented or active:

- SQL Declarative Schema loading from a single file or Directory Schema.
- PostgreSQL Shadow Database compilation through embedded or external PostgreSQL.
- Snapshot and Journal history for generated schema migrations.
- `diff` previews based on Migration Plan summaries and rename candidates.
- `generate` for schema-derived migrations.
- Custom Migrations via `create` or `generate --custom`.
- `dump` for SQL, JSON, and Directory Schema export.
- `drop` for removing local migration artifacts and Journal entries.
- Migration runner, status, checksum validation, and Down Migrations.
- `bootstrap` to author a baseline from an existing database, and `adopt` to validate and adopt an existing environment against it.
- Code generation from current Declarative Schema or Snapshot JSON for Rust, TypeScript, and Protobuf.

Still in progress:

- Validation workflows for comparing Declarative Schema, Snapshot history, migration artifacts, and live database shape.
- Full diff/render semantics for every PostgreSQL Catalog object represented by Dump.
- Declarative Schema generation for MySQL and SQLite.

## Related projects

`shki` stands on the shoulders of projects that explored these ideas first:

- [pgschema](https://github.com/pgschema/pgschema) — declarative, Terraform-style schema management for PostgreSQL.
- [jayy-lmao/sql-gen](https://github.com/jayy-lmao/sql-gen) — generating typed Rust code from a live database schema.
- [squirrel](https://github.com/giacomocavalieri/squirrel) — type-safe SQL code generation from introspected queries.
- [goose](https://github.com/pressly/goose) — a database migration tool. Supports SQL migrations and Go functions.
