# SQLite and MySQL support plan

Prioritize trustworthy migration execution, faithful schema handling, and then
declarative support. These foundations benefit existing users and prevent new
features from inheriting the current bugs.

Issue status reviewed on 2026-10-20: all 13 referenced issues were open.

## 1. Make migration execution trustworthy

| Order | Status      | Issue                                                                  | Deliverable                                                                                    |
| ----- | ----------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| 1     | in-progress | [#3 — Status validation](https://github.com/dk0d/shki/issues/3)        | Missing migration files and checksum mismatches become visible, actionable failures.           |
| 2     |             | [#15 — Connection ownership](https://github.com/dk0d/shki/issues/15)   | Every no-transaction migration retains one connection; session state cannot leak afterward.    |
| 3     |             | [#17 — Runner locking](https://github.com/dk0d/shki/issues/17)         | Overlapping deploys cannot execute the same migration twice.                                   |
| 4     |             | [#14 — MySQL failure recovery](https://github.com/dk0d/shki/issues/14) | Interrupted or partially committed migrations are tracked and require explicit reconciliation. |

Status establishes reliable reporting. Connection ownership supports
session-scoped locks. Locking makes recovery-state updates safe from competing
runners.

Split #14 into two deliverables:

1. Correct the atomicity claims and improve failure messages immediately.
2. Add durable execution state and recovery behavior after locking.

**Completion gate:** Test concurrent runners, failure after committed MySQL DDL,
process interruption, and recovery. Existing migration histories remain usable;
`status` stays read-only.

## 2. Make schemas survive a round trip

Start [#16 — Round-trip testing](https://github.com/dk0d/shki/issues/16) with the
first schema fix, then extend it throughout this phase.

| Order | Issue                                                                 | Deliverable                                                                                      |
| ----- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 5     | [#11 — Executable dumps](https://github.com/dk0d/shki/issues/11)      | Views, foreign keys, composite keys, and comments produce valid dialect-specific SQL.            |
| 6     | [#13 — Types and defaults](https://github.com/dk0d/shki/issues/13)    | Preserve native types, default values, nullability, and numeric ranges in generated code.        |
| 7     | [#12 — Metadata preservation](https://github.com/dk0d/shki/issues/12) | Preserve the behavior of indexes, generated columns, table options, and other supported objects. |

Each change should complete this loop:

> Create fixture → introspect → dump → restore → verify structure and behavior.

Comparing two Snapshots alone is insufficient: both could omit the same
information.

Break #12 into small end-to-end changes:

1. Index predicates, expressions, ordering, and collation.
2. Generated columns and table/column properties.
3. Triggers and foreign-key timing.
4. Additional MySQL objects and explicit unsupported-object reporting.

**Completion gate:** Dumps and bootstrap baselines recreate the declared
supported subset faithfully. Unsupported objects produce explicit diagnostics.
Document and exercise the supported database versions.

#16 is ongoing verification work, not a large testing project that must finish
before fixes start.

## 3. Deliver SQLite declarative support

Work through [#19](https://github.com/dk0d/shki/issues/19) in independently
verifiable increments:

1. **Compile and create:** disposable SQLite Shadow Database → Snapshot → initial
   migration → replay.
2. **Native alterations:** support selected changes SQLite can execute directly,
   with explicit rejection of unsupported changes.
3. **Table rebuilding:** preserve existing rows and supported dependent objects
   when an alteration requires recreating a table.
4. **Complete the workflow:** rename decisions, generated-SQL validation,
   Journal/Snapshot handling, schema codegen, and documentation.

**Completion gate:** Apply a migration to a populated database, verify data and
foreign-key integrity, then confirm a second diff is empty.

This is the first major feature milestone. SQLite's disposable database is
straightforward; correct rebuilding is the substantial work.

## 4. Deliver MySQL declarative support

Implement [#20](https://github.com/dk0d/shki/issues/20) similarly:

1. External disposable Shadow Database with ownership checks and cleanup.
2. Initial schema compilation and baseline replay.
3. Native MySQL alteration planning for the supported subset.
4. Integration with failure tracking, recovery, and generated-SQL validation.

**Completion gate:** A populated database reaches the intended shape, subsequent
diff is empty, and interrupted DDL remains recoverable.

Use an external MySQL server initially. An embedded distribution adds little
value to this milestone.

## 5. Add typed-query generation per dialect

Split [#21](https://github.com/dk0d/shki/issues/21) into SQLite and MySQL delivery
tracks.

Each needs:

- Its dialect's shadow compilation.
- Correct parameter/result typing and nullability.
- Correct sqlx executor/transaction types.
- Generated functions that compile and execute.

**Dependency correction:** Query generation needs a working shadow compiler and
type mappings, not the entire alteration/rebuild planner. SQLite query generation
can ship before MySQL declarative support is complete. This refines the current
issue-level blockers; GitHub dependencies have not yet been updated.

## Small fixes alongside the main work

- [#18 — Onboarding](https://github.com/dk0d/shki/issues/18): ship early; it
  currently sends users toward commands that cannot work.
- [#4 — Codegen summaries](https://github.com/dk0d/shki/issues/4): small independent
  improvement; fit it between larger changes.

## Decisions before the larger changes

1. **Recovery:** how operators reconcile failed MySQL migrations, including
   handwritten SQL whose intended outcome cannot be inferred automatically.
2. **SQLite locking:** how the runner remains exclusively owned across
   nontransactional segments without conflicting with its own transactions.
3. **Compatibility:** supported database versions and how existing
   Snapshots/history tables accommodate new metadata.

## Starting batch

Start with #3, #15, and #18, followed by #17. Begin #16 with the first #11
regression fix.

This ordering uses the audit and issue bodies. Runtime reproductions still need
to accompany implementation; writing this plan does not establish that the
acceptance criteria pass.
