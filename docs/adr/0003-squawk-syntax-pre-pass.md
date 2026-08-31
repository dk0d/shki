# ADR 0003: squawk-syntax for the Declarative Pre-Pass

- Status: Accepted
- Date: 2026-08-31
- Deciders: shki maintainers

## Context

The declarative pre-pass (`src/domain/sql/parse.rs`) — statement splitting,
statement classification for the planner, foreign-key deferral, and the
`CREATE INDEX CONCURRENTLY` rewrite (ADR 0002) — was hand-rolled token walking
over `squawk-lexer`. That produced gotcha-class bugs: the walker assumed a name
always follows `CONCURRENTLY`, so an unnamed `CREATE INDEX CONCURRENTLY ON t
(c)` silently lost its concurrent intent.

Two candidates were evaluated to replace it: `apache/datafusion-sqlparser-rs`
(generic multi-dialect typed AST) and `squawk-parser`/`squawk-syntax` (the
parser behind the squawk linter: a hand-written, Postgres-only,
rust-analyzer-style lossless rowan CST with a typed AST layer).

## Decision

Use **squawk-syntax** (same project and version train as the `squawk-lexer`
shki already depended on):

- **Error tolerance matches the architecture.** The Shadow Database is the
  authority on SQL validity; the pre-pass must never gate. `SourceFile::parse`
  always yields a tree — unparseable stretches become error nodes that keep
  their raw text and pass through to the shadow verbatim. sqlparser-rs fails
  hard on unknown syntax and would need a fallback wrapper bolted on.
- **Lossless CST enables surgical rewrites.** Stripping `CONCURRENTLY` or
  injecting an index name splices exact token ranges; every other byte of the
  user's SQL is preserved. sqlparser-rs would re-serialize (normalize) whole
  statements via `Display`.
- **Postgres-only, linter-grade coverage** (200+ commands) over generic
  multi-dialect coverage.

sqlparser-rs remains the right tool if shki ever needs cross-dialect semantic
understanding rather than light Postgres rewrites.

## Consequences

- Statement boundaries now come from the grammar: semicolons inside string
  literals, dollar-quoted function bodies, and `BEGIN ATOMIC` blocks no longer
  need bespoke handling.
- Structure questions ("is there a name between INDEX and ON?") are typed-AST
  accessors (`CreateIndex::index() -> Option<Index>`), not token-walk guesses.
- The pre-pass no longer errors on unterminated literals/comments — such input
  reaches the Shadow Database, which rejects it with Postgres's own error.
- A statement squawk's grammar doesn't recognize classifies as
  `Other`/`Raw` for planner ordering (previously any `CREATE ...` counted as a
  Create); it still applies, in source order.
- squawk's crates serve the linter first: sparse docs, possible API churn
  between versions. `squawk-lexer` remains a direct dependency only for query
  codegen tokenization, pinned to the same version as `squawk-syntax`.
