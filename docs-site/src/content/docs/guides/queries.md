---
title: Typed Queries
description: Generate type-safe Rust query functions from annotated PostgreSQL files.
---

:::caution[Alpha]
Query codegen is in alpha: annotation syntax and generated output may change
between releases.
:::

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
- **Schema-driven nullability.** Result columns and parameters get `T` vs
  `Option<T>` from the Declarative Schema's `NOT NULL` constraints, with
  explicit markers where inference cannot reach — see
  [Nullability](#nullability).
- **Named arguments.** A query may bind parameters as `$name` (e.g. `$email`)
  instead of positional `$1`, producing a self-documenting signature
  (`user_by_email(executor, email: String)`) rather than positional `arg1`. shki
  rewrites `$name` to `$n` before describing; the names exist only in the Rust
  signature. A single query must use one style or the other — mixing `$name` and
  `$1` is rejected.

  ```sql
  -- name: create_user :one
  INSERT INTO users (email, name, active)
  VALUES ($email, $name, $active)
  RETURNING *;

  -- name: users_by_status :many
  SELECT id, email FROM users
  WHERE active = $active AND created_at >= $since;

  -- Repeating a name binds one parameter to every occurrence:
  -- search(executor, term: String) — a single `term` argument.
  -- name: search :many
  SELECT * FROM users
  WHERE email ILIKE $term OR name ILIKE $term;

  -- name: rename_user :exec :tx
  UPDATE users SET name = $new_name WHERE id = $user_id;
  ```

  These generate:

  ```rust
  create_user(executor, email: String, name: String, active: bool) -> Result<Option<User>>
  users_by_status(executor, active: bool, since: DateTime<Utc>) -> Result<Vec<UsersByStatusRow>>
  search(executor, term: String) -> Result<Vec<User>>
  rename_user(tx: &mut Transaction<'_, Postgres>, new_name: String, user_id: i64) -> Result<u64>
  ```

  Parameter order follows first appearance in the SQL. A `$name` inside a
  string literal, comment, or dollar-quoted body is left alone — only real
  placeholders are rewritten.

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

## Nullability

Postgres' describe output does not report nullability, so shki infers it from
the Declarative Schema — its source of truth — falling back to sqlx's
describe-time analysis, and gives you explicit markers for the cases neither
can prove.

| Where                                                | Rust type                                                     |
| ---------------------------------------------------- | ------------------------------------------------------------- |
| Result column traced to a schema column              | Schema `NOT NULL` → `T`; nullable (or outer join) → `Option<T>` |
| Result column with no table origin (expression, `UNION`) | `Option<T>` unless proven; force with `AS "name!"` / `AS "name?"` |
| Parameter written whole into a column (`VALUES` / `SET`) | Inferred from that column: nullable → `Option<T>`               |
| Any other parameter                                  | `T`; mark `?name` for `Option<T>`                              |

### Result columns

A column traced to a base-table column honors the schema's `NOT NULL`
constraint (`T` vs `Option<T>`); the schema is authoritative unless the query
itself makes the column nullable (e.g. the outer side of a join). Anything the
schema cannot speak to — expressions, function results — defaults to
`Option<T>` unless sqlx proves otherwise.

Where inference cannot reach — e.g. `UNION` output columns, which lose their
table origin — force it with an sqlx-style alias marker: `AS "id!"` forces
`T`, `AS "note?"` forces `Option<T>`. The marker is stripped from the field
name.

```sql
-- name: all_account_ids :many
SELECT id AS "id!" FROM users UNION ALL SELECT id FROM service_accounts;
```

### Parameters

A parameter written whole into a nullable column — `INSERT INTO t (a) VALUES
($a)` or `UPDATE t SET a = $a`, including `ON CONFLICT ... DO UPDATE SET` — is
inferred nullable automatically: the generated argument is `Option<T>`,
binding SQL `NULL` when `None`.

```sql
-- annotation is a nullable column, so this generates
-- upsert_annotation(executor, id: i64, name: String, annotation: Option<String>)
-- name: upsert_annotation :exec
INSERT INTO attributes (id, name, annotation)
VALUES ($id, $name, $annotation)
ON CONFLICT (id) DO UPDATE SET annotation = EXCLUDED.annotation;
```

Inference only reaches parameters that are the entire value for a column.
Everywhere else — comparisons, expressions, casts — a `?` prefix on a named
parameter (`?name` instead of `$name`) marks it nullable explicitly. Marking
any occurrence marks the parameter: `$status` and `?status` in one query are
the same (nullable) argument. Write the SQL so `NULL` means what you want
(e.g. an optional filter):

```sql
-- name: users_by_optional_status :many
SELECT * FROM users
WHERE status = ?status OR $status::user_status IS NULL;
```

```rust
users_by_optional_status(executor, status: Option<UserStatus>) -> Result<Vec<User>>
```

Notes:

- Only plain arguments can be nullable — `?limit`/`?offset` and keyset cursor
  parameters are rejected.
- Positional (`$1`) queries have no nullable form; use named parameters.
- A `?` not directly followed by an identifier (like the JSONB `data ? 'key'`
  operator) is left alone — keep a space after operator uses of `?` so they
  aren't read as a parameter.
- `INSERT` without an explicit column list is not inferred; add the column
  list or use `?name`.

## Limitations

- **PostgreSQL only.** Describe-based typing relies on PostgreSQL; MySQL/SQLite
  query codegen is not implemented.
- **Rust/sqlx only.** TypeScript/Protobuf query output is not implemented (schema
  [codegen](../../guides/codegen/) covers those for types).
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
