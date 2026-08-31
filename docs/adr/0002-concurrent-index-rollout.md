# ADR 0002: Concurrent Index Rollout from the Declarative Schema

- Status: Accepted
- Date: 2026-08-31
- Deciders: shki maintainers

## Context

Ordinary `CREATE INDEX` takes a write-blocking lock on the table for the
duration of the build — unacceptable on a large, live table. PostgreSQL's
answer is `CREATE INDEX CONCURRENTLY`, which refuses to run inside a
transaction block, while `shki migrate` runs each migration inside a single
transaction. shki already had the runtime half of the answer: the
`shki:no-transaction` directive plus `--> +statement` segments run a migration
outside any transaction, one statement at a time. But the generator half was
missing — a diff that added an index always rendered a plain, locking
`CREATE INDEX`, and review tooling (e.g. coderabbit) flagged every generated
index migration for it. Users had to hand-write the online rollout as a Custom
Migration every time.

Two prior-art positions framed the design:

- **drizzle-kit** renders `.concurrently()` into migration SQL but its migrator
  wraps every migration in a transaction, so the statement fails at runtime
  (drizzle-orm #860); there is no per-migration transaction opt-out. Strategy
  is expressible but not executable.
- **drizzle's "strict migrations" stance** (no `IF NOT EXISTS` / `DO` guards in
  generated DDL): in a linear, transactional migration ledger a guard only
  hides drift — `IF NOT EXISTS` matches on the index *name*, so an out-of-band
  index with the same name but a different definition passes silently.

## Decision

1. **Declaring `CREATE INDEX CONCURRENTLY` in the Declarative Schema is the
   opt-in signal.** No config key, no flag — the SQL the user already writes
   carries the intent.

2. **`shki generate` splits the plan into two migrations** when any diffed
   index creation is declared concurrent: first the ordinary transactional
   migration with every other change (including drops of indexes being
   redefined), then a `-- shki:no-transaction` migration that builds each
   concurrent index with `CREATE INDEX CONCURRENTLY IF NOT EXISTS`, one
   `--> +statement` segment per index. Down migrations mirror this with
   `DROP INDEX CONCURRENTLY IF EXISTS` under the same directive. If the only
   change is concurrent indexes, only the no-transaction migration is written.

3. **The split requires interactive confirmation.** Declining fails the whole
   generation with nothing written; a non-interactive run fails with an error
   explaining why. Generation output changing shape (two files instead of one)
   is something the user must consciously accept.

4. **Generated DDL is strict everywhere else.** No `IF NOT EXISTS` /
   `IF EXISTS` in transactional migrations — a name collision should fail
   loudly and surface drift (agreeing with drizzle). The guards appear only in
   the no-transaction migration, where they are load-bearing: a partial failure
   leaves earlier segments committed and the migration unrecorded, so the next
   run replays the file from the top and must be idempotent.

5. **`concurrently` is a creation strategy, not schema state.**
   - `Index::concurrently` is `#[serde(skip)]`: it never enters Snapshots and
     never participates in index diffing, so adding or removing the keyword on
     an already-existing index diffs as no change.
   - The declarative planner strips `CONCURRENTLY` before applying the schema
     to the Shadow Database (it cannot run in the apply's implicit transaction,
     and PostgreSQL catalogs would not record it anyway); the compiler re-marks
     the introspected indexes from the recorded names
     (`mark_concurrent_indexes`). An unnamed concurrent index gets a
     PostgreSQL-convention name injected (`{table}_{columns}_idx`, expressions
     as `expr`, no collision dedup) so the intent survives the round trip.
   - `DiffStatement::CreateIndex` / `DropIndex` carry **no**
     `concurrently` / `if_not_exists` / `if_exists` fields. The renderers
     derive execution style from `Index::concurrently` (or `prev.concurrently`
     for drops) and emit `CONCURRENTLY IF NOT EXISTS` /
     `CONCURRENTLY IF EXISTS` as one inseparable, Postgres-only pair — the
     incoherent combinations (concurrent without guard, guard inside a
     transaction) are unrepresentable.

## Consequences

- A declared concurrent index costs one extra migration file per generate.
  Every generated migration records its own Snapshot: the transactional
  migration gets the intermediate state (desired minus the not-yet-built
  indexes, with its own id), the no-transaction migration gets the desired
  state, and `prev_id` chains baseline → intermediate → desired.
- Strict DDL means re-running a *transactional* migration against a database
  that already has the index fails — intended, that failure is the drift
  signal. Only the isolated no-transaction step is idempotent, and a failed
  concurrent build can still leave an `INVALID` index that `IF NOT EXISTS`
  would keep; the troubleshooting guide covers the `pg_index.indisvalid`
  cleanup.
- `CONCURRENTLY` is Postgres-only by construction in the renderer; MySQL and
  SQLite render plain strict DDL regardless of the flag.
- New execution-affecting behaviors should follow the same pattern: declared in
  SQL, excluded from Snapshot identity, executed via a directive.
