# ADR 0001: Typed Query Codegen

- Status: Proposed (feature-gated behind `querygen`)
- Date: 2026-06-16
- Deciders: shki maintainers

## Context

shki manages a Declarative Schema and already generates language types from a
`Snapshot` (Rust structs/enums, TypeScript, Protobuf) via the `CodeGenerator` /
`CodeWriter` traits in `src/domain/codegen/`. That path is purely static:
`Snapshot -> types`, with no database involved at generation time.

We want a second capability, inspired by
[squirrel](https://github.com/giacomocavalieri/squirrel) (Gleam): a user drops
SQL queries into a directory and shki generates **type-safe functions that wrap
those queries**, using `sqlx` as the execution runtime. Each query becomes a
function with typed parameters and a typed result.

Until the type/runtime contract and integration coverage are complete, this
capability is excluded from default builds and must be enabled with Cargo's
`querygen` feature.

Two observations make this a good fit for shki specifically rather than a
generic codegen exercise:

1. **shki already owns a describe target.** squirrel's core technique is to ask
   Postgres for a query's parameter and result types via the extended-query
   protocol (`Parse` + `Describe`, no execution) instead of parsing SQL itself.
   shki's `with_embedded_shadow_pool` (`src/domain/compiler.rs`) already stands
   up an embedded Postgres, applies the Declarative Schema, and yields a
   `sqlx::Pool<Postgres>`. That is exactly the describe target, available with no
   new infrastructure and no live production database required at the user's
   build time.

2. **shki already knows the schema's types.** The Rust generator keeps a
   `table_name -> RustStruct` map (PascalCase, singular) and an enum map, with
   rename/pattern/override config. When a query returns columns that correspond
   to a known table or enum, the generated function should return the
   _already-generated_ schema type rather than minting a parallel ad-hoc row
   type. This keeps the two codegen outputs coherent and idiomatic.

## Decision Drivers

- Correctness of result/parameter types across arbitrary SQL (joins,
  expressions, function calls), not just trivial `SELECT *`.
- No requirement for a live production database at the user's compile time.
- Output that feels native: reuse schema structs/enums; minimal `Option` noise.
- Reuse of existing codegen machinery (type mapping, writers, preview, config).
- Consistency with shki's existing Postgres-first focus.

## Decision

### 1. Type source: describe against the shadow database

Type information for both parameters and result columns is obtained by
**describing each query against the embedded/external shadow database**, after
the Declarative Schema has been applied to it — not by statically parsing the
SQL. This reuses `with_embedded_shadow_pool` and inherits accurate typing for
expressions, joins, and function calls.

Rejected alternative — _static SQL parse_: no DB needed, but cannot type
expressions or resolve `SELECT *` reliably, and would duplicate Postgres's
planner. shki already pays the shadow-DB cost during `diff`/`generate`, so the
marginal cost here is low.

Rejected alternative — _generate `sqlx::query!`/`query_as!` macro calls_: those
macros re-verify against `DATABASE_URL` at the user's compile time, which
duplicates the type-checking shki just did and reintroduces the live-DB-at-build
dependency we are trying to remove. Instead, generated code uses sqlx's
**runtime API** (`sqlx::query(...).bind(...)` + `FromRow`/positional decode).
shki owns the type-checking; generated code is "dumb" and trusts it.

### 2. Reuse schema-derived structs and enums where possible

`RowDescription` reports, per result column, the **source table OID and column
attnum** when the column maps directly to a table column. The query generator
will:

1. Group the result columns and test whether they correspond, in full and in
   order, to a single known table's columns (matched via table OID + attnum
   against the `Snapshot`). If so, return that table's existing `RustStruct`
   (respecting `struct_renames` / `struct_pattern`).
2. For individual columns whose type OID is a known enum, use the existing
   `RustEnum` name.
3. Otherwise, synthesize a per-query row struct (e.g. `GetActiveUsersRow`),
   reusing the existing `DataType -> Rust` mapping (`sql_type_to_rust`) and the
   same field-naming / serde / `FromRow` conventions.

This means a query like `SELECT * FROM users WHERE id = $1` returns
`Option<User>`, reusing the generated `User` struct, while a projection or join
gets a purpose-built row type that still references shared enum types.

### 3. Nullability inference via the Declarative Schema

`RowDescription` does **not** carry nullability — this is the hard part and the
main quality risk (it is why sqlx forces `as "x!"` overrides). The Postgres
describe output is therefore used only to identify _which schema object each
result column comes from_ (its source table OID + column attnum); nullability
itself is inferred from the **Declarative Schema** (the `Snapshot`), which is
shki's source of truth and a strict advantage over describe-only tools like
squirrel.

- Column traces to a base table column (table OID + attnum) → look up the column
  in the `Snapshot` and honor its `NOT NULL` constraint → `T` vs `Option<T>`.
  The describe result OID and the schema agree on the base type; the schema
  decides nullability.
- Where the schema cannot speak to a column — an expression, function result, or
  a column made nullable by an outer join even though its base column is
  `NOT NULL` — shki defaults to `Option<T>` rather than guessing non-null.
- Per-column override annotations provide the escape hatch for the cases the
  schema cannot prove (e.g. forcing non-null on an expression the author knows is
  total).

### 4. Query file convention and cardinality: sqlc-style annotations

Queries are authored in `*.sql` files under a configured directory (default
`db/queries/`). Each query is annotated, sqlc-style:

```sql
-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;

-- name: active_users :many
SELECT id, email FROM users WHERE active = true;

-- name: deactivate_user :exec
UPDATE users SET active = false WHERE id = $1;

-- name: item_from_id :one
SELECT * FROM items WHERE id = $id;
```

- `:one` → `Result<Option<Row>>`
- `:many` → `Result<Vec<Row>>`
- `:exec` → `Result<u64>` (rows affected)
- `:batch` → a paginated `:many`: returns `Result<Page<Row>>` and takes
  pagination input. See **Pagination (`:batch`)** below.

The fourth example (`item_from_id`) uses a **named argument** (`$id`) in the SQL
body rather than a positional `$1`. Named arguments are orthogonal to
cardinality — any query may use them. See **Named arguments** below.

The function name comes **directly from the annotation `name:`** — the author
names the query and that is the function name, verbatim. shki does not
synthesize, prepend, or suggest any prefix (no SQL verb, no `fetch_`/`get_`).
Auto-prefixing the SQL verb was considered and rejected: it produces awkward
collisions when the name already implies an action (e.g. an `UPDATE` named
`deactivate_user` becoming `update_deactivate_user`).

- The annotation `name:` is a language-agnostic identifier, normalized to
  `snake_case` for Rust (and per-language elsewhere) via the existing `heck`
  casing in codegen; authors may write it in any case.
- The name is reused in PascalCase to name synthesized row structs —
  `active_users` → `ActiveUsersRow`.

Rejected alternative — _squirrel-style one-query-per-file, filename = function
name_: simpler and magic-free, but cardinality must then be guessed or inferred,
and packing related queries into one file is convenient. Explicit annotations
remove the guesswork that squirrel users most often wish away.

#### Named arguments

A query may bind parameters by name as `$name`, extending Postgres' own `$1`
placeholder syntax with an identifier instead of a number. This makes the
generated function signature self-documenting (`item_from_id(executor, id: i32)`
rather than `arg1`) and lets a parameter be referenced more than once without
repeating a positional index.

Postgres' protocol only understands numbered placeholders, so before describing
a query shki **rewrites** each distinct `$name` to a `$n` (in first-appearance
order; repeated `$name` references collapse to the same `$n`), describes the
rewritten SQL to type the parameters, then maps the resolved types back onto the
names. The rewritten positional SQL is what the generated function executes; the
names exist only in the Rust signature.

- A query uses one style or the other — mixing `$name` and `$1` in a single
  query is rejected, to keep the rewrite unambiguous.
- Positional `$1` queries are unchanged and keep generating positional `argN`
  parameters, so named arguments are additive and opt-in per query.
- Names are normalized with the same `heck` casing as the rest of codegen.
- `$name` must be distinguished from dollar-quoted strings (`$tag$...$tag$`): a
  `$ident` run is a parameter only when it is **not** immediately followed by
  another `$`. The scanner also skips quoted strings and comments so `$name`
  inside them is left untouched.

#### Pagination (`:batch`)

`:batch` marks a `:many` query that returns one page of a larger result set, in
one of two modes:

- **Limit/offset** — the query carries an `OFFSET` bind placeholder (paired with
  `LIMIT`, e.g. `LIMIT $limit OFFSET $offset`). shki generates a shared
  `Pagination { limit, offset }` struct, the function takes it by reference
  (`page: &Pagination`) and binds its fields to those placeholders, and returns
  `Result<Page<Row>>`. `Pagination` and `Page<T>` are emitted **once** and reused
  by every limit/offset batch query.
- **Cursor (keyset)** — selected by a `:keyset` modifier listing the cursor's
  bind parameters, e.g. `:batch :keyset $1 $2` for:

  ```sql
  -- name: events_after :batch :keyset $1 $2
  SELECT * FROM events
  WHERE (id, created_at) > ($1, $2)
  ORDER BY id, created_at
  LIMIT $3;
  ```

  The keyset parameters are typed from describe and become the cursor key: a
  single type for one key, or a tuple in annotation order for several (here
  `(i32, DateTime<Utc>)`). The function takes a shared
  `cursor: &CursorPagination<K>` and binds `cursor.key` (or `cursor.key.0`,
  `cursor.key.1`, …) to those placeholders; any non-keyset parameters (e.g. the
  `LIMIT`) stay ordinary arguments. `CursorPagination<K>` is emitted once.

The mode is explicit, not inferred: `:keyset` ⇒ cursor, otherwise an `$offset`
placeholder ⇒ limit/offset; a `:batch` query with neither is an error.

Each pagination type is emitted only when a query needs it (`Pagination`/`Page`
for limit/offset, `CursorPagination` for keyset).

**Not yet implemented:** cursor mode currently returns `Result<Vec<Row>>` and
does not derive the *next* cursor from the last row (that requires mapping
keyset params to result columns); `CursorPagination`'s `next`/`prev` fields are
caller-managed for now. Producing the next cursor — and a richer keyset `Page` —
is the remaining follow-up.

### 5. Dialect scope: PostgreSQL first

Describe-based typing is cleanest on Postgres, which is where the Declarative
Schema and shadow DB are already focused. MySQL/SQLite describe semantics differ
enough to be tracked as separate, later work.

## Architecture

This is a **second front-end to the existing codegen subsystem**, not a parallel
system. Today: `Snapshot -> types`. New: `query files -> described types ->
typed functions`, sharing the type mapping, writers, preview, and config.

```text
db/queries/*.sql
  -> parse annotations (name, cardinality)
  -> apply Declarative Schema to shadow DB (existing compiler path)
  -> Parse + Describe each query  => param OIDs, RowDescription
  -> resolve types via Snapshot:
       reuse RustStruct / RustEnum where columns map to known objects,
       else synthesize a per-query Row struct
       nullability from Snapshot NOT NULL + describe metadata
  -> CodeGenerator / CodeWriter (reuse Rust/TS writers, preview, output modes)
```

New config lives alongside `CodegenConfig`: a queries source directory and any
query-specific overrides. Multi-language reach (TS, etc.) comes "for free" via
the existing writer trait, though only Rust/sqlx is in scope for the first cut.

## Consequences

Positive:

- Type-safe query wrappers without a live production DB at the user's build time.
- Accurate typing for arbitrary SQL, reusing shki's existing shadow DB.
- Output coheres with schema codegen by reusing its structs/enums.
- Better nullability than describe-only tools, thanks to the `Snapshot`.

Negative / risks:

- Nullability inference is the make-or-break for output quality and must be
  validated early.
- Adds shadow-DB startup latency to the query-codegen path (already paid
  elsewhere, but new for users who only want query codegen).
- Per-query row-struct naming, parameter naming, and collision handling need a
  clear, documented scheme.
- sqlx runtime-API codegen couples generated output to a sqlx major version.

## Open Questions

- How aggressively to collapse "looks like table X" projections onto the table
  struct vs always synthesizing a row type for projections.
- Where query codegen sits in the CLI: a `codegen queries` subcommand vs folding
  into the existing `codegen` flow.
- Override syntax for forcing nullability / custom types per column.
- Keyset next-cursor extraction: deriving the next cursor from the last row
  requires mapping keyset bind params to their result columns (e.g. by name or
  an extended `:keyset` syntax). Until then keyset `:batch` returns `Vec<Row>`
  and the caller manages `next`/`prev`.
- The keyset `Page` shape once next-cursor extraction lands (e.g. a
  `KeysetPage<Row, K>` carrying rows + the next `CursorPagination<K>`).
- Whether to allow naming positional `$1` parameters (e.g. via an annotation)
  for queries that cannot or prefer not to use `$name` placeholders.

## Follow-ups

- Vertical-slice prototype: one `:one` query, described against the shadow pool,
  generating a Rust function returning `Option<User>` by reusing the existing
  `User` struct — primarily to derisk nullability inference and type reuse.
- Named arguments (`$name` → `$n` rewrite) as a slice on top of the core
  positional support; additive and independently testable.
- Keyset next-cursor extraction and a richer keyset `Page` (the keyset input
  side — `:keyset` annotation, tuple `CursorPagination<K>`, and binding — is
  implemented; output-side next-cursor remains).
