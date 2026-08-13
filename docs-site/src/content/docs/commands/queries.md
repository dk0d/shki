---
title: shki queries
description: Generate type-safe Rust query functions from annotated SQL.
---

```bash
shki queries [--sources PATH] [--output PATH] [--format file|module|modules] [--preview]
```

Alias: `shki q`. PostgreSQL only. Turns annotated `*.sql` files into typed Rust
functions backed by `sqlx`, resolving parameter and result types by describing
each query against the Shadow Database — so no live database is needed at your
compile time.

```sql
-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;
```

becomes a function returning `Result<Option<User>>`, reusing the struct
[`shki codegen`](/shki/commands/codegen/) generates for `users`.

## Options

| Flag                                   | Purpose                                                                    |
| -------------------------------------- | ---------------------------------------------------------------------------- |
| `-s, --sources <PATH>`                 | Annotated SQL file or directory. Default `<root>/queries`.                  |
| `-o, --output <PATH>`                  | Output file. Prints to stdout when omitted.                                 |
| `-f, --format <file\|module\|modules>` | Output layout, shared with `[codegen]`.                                     |
| `--models <PATH>`                      | Rust module path for schema types (e.g. `crate::models`). Usually derived.  |
| `--preview`                            | Print the generated code without writing.                                   |
| `--shadow-database-url <URL>`          | External Shadow Database.                                                   |
| `--pg-version <14…18>`                 | Embedded PostgreSQL major version.                                          |

## Examples

```bash
shki queries                                          # default sources, to stdout
shki queries --sources db/queries --output src/queries.rs
shki queries --sources db/queries --preview
```

## Annotations at a glance

| Tag        | Returns                                             |
| ---------- | ----------------------------------------------------- |
| `:one`     | `Result<Option<Row>>`                                |
| `:many`    | `Result<Vec<Row>>`                                   |
| `:exec`    | `Result<u64>` (rows affected)                        |
| `:batch`   | Paginated `:many` — `Result<Page<Row>>`              |
| `:tx`      | Modifier: require a `sqlx::Transaction` executor     |
| `:keyset`  | Modifier on `:batch`: cursor/keyset pagination       |

Parameters can be positional (`$1`) or named (`$email`), but not both in one
query.

## Notes

- The Shadow Database starts for the describe step, so this pays the same
  startup cost as `diff`/`generate`.
- Rust/sqlx output only; TypeScript and Protobuf are schema-`codegen` targets.

**Full details: [Typed Queries guide](/shki/guides/queries/).**
