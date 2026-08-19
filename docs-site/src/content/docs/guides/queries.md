---
title: Typed Queries
description: Generate type-safe Rust query functions from annotated PostgreSQL files.
---

:::note
PostgreSQL only. Describe-based typing relies on PostgreSQL; MySQL/SQLite query
codegen is not implemented.
:::

`queries` is enabled by default:

```bash
shki queries
```

`queries` turns annotated `*.sql` files into type-safe Rust functions backed by
`sqlx`. Each query becomes a function with typed parameters and a typed result.
Unlike `sqlx::query!`, the types are resolved at generation time by **describing**
each query against the Shadow Database (the same embedded/external PostgreSQL
used by `diff`/`generate`), so **no live production database is required at your
compile time** and the generated code uses sqlx's runtime API — it is not
re-checked against `DATABASE_URL`.

```bash
# Default: read <root>/queries, print to stdout
shki queries

# Read a file or directory, write to a file
shki queries --sources db/queries --output src/queries.rs

# Preview without writing
shki queries --sources db/queries --preview
```

## Annotations

Annotate each query with a name and cardinality, sqlc-style. The `name:` is the
function name verbatim (normalized to `snake_case`); no prefix is added.

```sql
-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;

-- name: active_users :many
SELECT id, email FROM users WHERE active = true;

-- name: deactivate_user :exec
UPDATE users SET active = false WHERE id = $1;

-- name: deactivate_user_in_tx :exec :tx
UPDATE users SET active = false WHERE id = $1;

-- name: user_by_email :one
SELECT * FROM users WHERE email = $email;
```

Cardinality controls the return shape:

| Tag      | Returns                                                  |
| -------- | -------------------------------------------------------- |
| `:one`   | `Result<Option<Row>>`                                    |
| `:many`  | `Result<Vec<Row>>`                                       |
| `:exec`  | `Result<u64>` (rows affected)                            |
| `:batch` | A paginated `:many` — `Result<Page<Row>>` (limit/offset) |

## Features

- **Reuses schema types.** When a query's result columns map, in full and in
  order, to a known table, the function returns that table's generated struct
  (e.g. `Option<User>`) instead of a parallel row type; columns whose type is a
  known enum reuse the generated enum. Projections and joins get a synthesized
  per-query row struct named from the query (`active_users` → `ActiveUsersRow`).
  The generated module imports these types with a `use` path derived from your
  output layout (override with `models`).
- **Schema-driven nullability.** `RowDescription` does not report nullability, so
  it is inferred from the Declarative Schema: a column traced to a base-table
  column honors its `NOT NULL` constraint (`T` vs `Option<T>`); anything the
  schema cannot prove (expressions, function results, outer-join columns)
  defaults to `Option<T>`.
- **Named arguments.** A query may bind parameters as `$name` (e.g. `$email`)
  instead of positional `$1`, producing a self-documenting signature
  (`user_by_email(executor, email: String)`) rather than positional `arg1`. shki
  rewrites `$name` to `$n` before describing; the names exist only in the Rust
  signature. A single query must use one style or the other — mixing `$name` and
  `$1` is rejected.
- **Transactions.** Add `:tx` to require a
  `&mut sqlx::Transaction<'_, sqlx::Postgres>` instead of a generic executor,
  e.g. `-- name: deactivate_user :exec :tx`. The generated wrapper executes only
  through that transaction.
- **Pagination (`:batch`).** Two explicit modes:
  - **Limit/offset** — a query carrying a `LIMIT $limit OFFSET $offset`
    placeholder takes a shared `Pagination { limit, offset }` by reference and
    returns `Result<Page<Row>>`. `Pagination`/`Page<T>` are emitted once and
    reused.
  - **Cursor/keyset** — selected by a `:keyset` modifier mapping cursor bind
    parameters to selected fields (e.g.
    `-- name: events_after :batch :keyset $1=id $2=created_at`). The function
    takes a `cursor: &CursorPagination<K>` (where `K` is the keyset type, a tuple
    for multiple keys) and returns `KeysetPage<Row, K>` with the next cursor
    derived from the final row.

## Limitations

- **PostgreSQL only.** Describe-based typing relies on PostgreSQL; MySQL/SQLite
  query codegen is not implemented.
- **Rust/sqlx only.** TypeScript/Protobuf query output is not implemented (schema
  [codegen](/shki/guides/codegen/) covers those for types).
- **Generated query rows always derive `sqlx::FromRow`**, regardless of the
  `[codegen] sqlx` toggle, since they are decoded by sqlx.
- **Unsupported runtime mappings fail generation.** Types that the Rust schema
  generator renders as `String` but sqlx cannot decode as `String` (such as
  `NUMERIC`, ranges, network, geometric, and interval types) require a compatible
  `[codegen.type_overrides]` entry.
- The Shadow Database is started for the describe step, so query codegen pays the
  same startup cost as `diff`/`generate`.

## Configuration

Configure query generation in `[queries]`:

```toml
[queries]
sources = "db/queries"          # SQL file or directory (default: <root>/queries)
output = "src/db/queries.rs"     # output file; prints to stdout if omitted
format = "file"                  # output layout, as in [codegen]
# models is optional — see below. By default it is derived from the
# codegen/queries output paths, e.g. with [codegen] output = "src/db/models.rs"
# the generated module imports `use super::models::*;`.
```

| Option    | Purpose                                                                                                                                                                                                                                                                                                                                                                                                         |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sources` | SQL file or directory of annotated `*.sql` queries. Relative paths resolve from `root`. Default `<root>/queries`.                                                                                                                                                                                                                                                                                               |
| `output`  | Output file for generated Rust. Prints to stdout when omitted. Relative paths resolve from `root`.                                                                                                                                                                                                                                                                                                              |
| `format`  | Output layout: `file`, `module`, or `modules` (shared with `[codegen]`).                                                                                                                                                                                                                                                                                                                                        |
| `models`  | Rust module path imported as `use <path>::*;` so generated functions can name your schema structs/enums. **Optional** — derived from the `[codegen]`/`[queries]` output paths when unset (sibling files share a directory, so e.g. `models.rs` + `queries.rs` → `super::models`). Set it (e.g. `crate::models`) only to override that for non-standard layouts; it must be a Rust module path, not a file path. |

The schema type mapping, naming/rename config, output modes, and `--preview` are
shared with `[codegen]`;
