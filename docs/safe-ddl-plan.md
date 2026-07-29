# Safe DDL generation (Postgres)

Status: proposed (2026-07-14)

## Problem

The diff generator emits blocking DDL for operations that have well-known
non-blocking expansions. Example: a nullable→NOT NULL diff generates

```sql
ALTER TABLE "t" ALTER COLUMN "c" SET NOT NULL;
```

which takes an ACCESS EXCLUSIVE lock and full-scans the table while holding
it. On a large, live table that's an outage.

## Proposal

Teach the generator to expand risky ops into stepped, non-blocking forms,
separated by breakpoints so the executor commits each step in its own
transaction. **This is the load-bearing constraint**: run in a single
transaction, the `NOT VALID` lock is held through the `VALIDATE` scan and the
pattern is strictly worse than the naive statement.

### SET NOT NULL

```sql
ALTER TABLE "t" ADD CONSTRAINT "t_c_nn" CHECK ("c" IS NOT NULL) NOT VALID;
--> breakpoint
ALTER TABLE "t" VALIDATE CONSTRAINT "t_c_nn";  -- SHARE UPDATE EXCLUSIVE; scan without blocking writes
--> breakpoint
ALTER TABLE "t" ALTER COLUMN "c" SET NOT NULL;  -- PG 12+: reuses validated check, no scan
--> breakpoint
ALTER TABLE "t" DROP CONSTRAINT "t_c_nn";
```

### Same family, same flag

- Add foreign key → `ADD CONSTRAINT ... NOT VALID` + separate `VALIDATE CONSTRAINT`
- Create index → `CREATE INDEX CONCURRENTLY` (requires running outside a
  transaction — same executor prerequisite as above)

## Design decisions

- Config: `[migrations] safe_ddl = true`, postgres-only. **Default on** for
  generation — generated files are editable, so a dev with a tiny table can
  delete three lines; the reverse mistake (blocking scan on a big table) is
  the 3am page.
- Constraint names derived deterministically from table + column
  (`{table}_{column}_nn`) so re-generation is stable and down migrations can
  reference them.
- PG < 12 doesn't reuse the validated check for `SET NOT NULL` (step 3 still
  scans). Document PG 12+ as the floor rather than version-gating.
- Executor: `breakpoints` already exists; verify each segment commits in its
  own transaction and that `CREATE INDEX CONCURRENTLY` segments run with no
  surrounding transaction at all.

## Origin

Found reviewing scan-api-rs migration `0003_algo-id-not-null.sql`, which
shipped the naive form (fine there — small tables, applied off-peak).
