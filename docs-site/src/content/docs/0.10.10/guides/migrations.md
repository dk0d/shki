---
title: Migrations
description: Preview, generate, hand-write, apply, adopt, and roll back migrations.
slug: 0.10.10/guides/migrations
---

## Preview Declarative Schema changes

```bash
shki diff
```

`diff` compiles the Declarative Schema, compares it to the latest schema Snapshot
in the Journal, and prints a preview of the Migration Plan. The preview
summarizes object-level changes and rename candidates. It does not print
generated SQL and does not write files.

## Generate a schema-derived migration

```bash
shki generate add_users_table
```

`generate` writes:

* `migrations/<migration>.sql`
* `migrations/_meta/<migration>.snapshot.json`
* `migrations/_meta/_journal.json`

Use `--down` (or `migrations.generate_down = true`) to write a Down Migration:

```bash
shki generate add_users_table --down
```

If Shki detects possible renames, `generate` prompts before rendering the
migration. Choosing a rename replaces drop/create statements with rename
statements where supported.

## Create a Custom Migration

Use Custom Migrations for hand-written SQL — data backfills, operational SQL, or
schema changes the Declarative Schema can't express. Any schema-shape changes
they make are still tracked (see below), so the Declarative Schema and Snapshot
chain stay in sync.

```bash
shki create backfill_user_emails --with-down
```

or:

```bash
shki generate backfill_user_emails --custom
```

Seed a Custom Migration with inline SQL:

```bash
shki create add_users_index \
  --sql 'CREATE INDEX idx_users_email ON users(email);'
```

Seed a Custom Migration from a file:

```bash
shki create add_audit_table --sql-file ./sql/add_audit_table.sql
```

Custom Migrations are executable artifacts recorded in the Journal. Their SQL
isn't final at creation time, so no Snapshot is written then — but the next time
a diff is needed (`diff` or `generate`), Shki replays any not-yet-snapshotted
migrations on a Shadow Database, introspects the result, and records a Snapshot
for each. This keeps the Snapshot chain complete: if a Custom Migration changes
the schema shape, that change is captured in the baseline so the next generated
migration accounts for it (and won't re-emit DDL the Custom Migration already
applied).

### Running outside a transaction

Each migration runs inside a single transaction, so a failure rolls the whole
file back. Some PostgreSQL statements refuse to run that way — most commonly
`CREATE INDEX CONCURRENTLY`, the non-blocking way to add an index to a large,
live table. Declare the index `CONCURRENTLY` in the Declarative Schema and
[`shki generate`](../../commands/generate/#concurrent-indexes) writes this
rollout for you; for hand-written migrations, add the `shki:no-transaction`
directive to opt a migration out:

```sql
-- shki:no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_scan_captured_at ON scan (captured_at);
--> +statement
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_scan_profile ON scan (profile_id);
```

Each `--> +statement` segment is then sent on its own, outside any transaction.
Two consequences:

* **The migration must be idempotent.** A failure part-way leaves earlier
  segments committed and the migration unrecorded, so the next `shki migrate`
  replays the file from the top. Write `IF NOT EXISTS` and the like.
* **A failed concurrent index build leaves an `INVALID` index behind**, which
  `IF NOT EXISTS` would then silently keep. Check `pg_index.indisvalid`,
  `DROP INDEX` the invalid one, and re-run. Shki says so in the error when a
  `CONCURRENTLY` statement fails.

Directives are comments, and checksums are computed on comment-stripped SQL, so
adding one to a migration that has already been applied elsewhere does **not**
invalidate its checksum or require a Journal edit. An unrecognized `shki:`
directive is a hard error rather than a silent no-op, so a typo surfaces before
it reaches production.

## Apply migrations

```bash
shki migrate
```

`migrate` applies pending SQL files and records applied checksums in the live
database migration table. It does not mutate local Snapshot files or the Journal.
With no mode, `migrate` applies all pending migrations.

Apply a limited number of pending migrations:

```bash
shki migrate steps 2
```

Apply through a specific pending migration name:

```bash
shki migrate to 0003_add_users
```

Preview what would be applied without changing the database:

```bash
shki migrate --dry
shki migrate --dry steps 1
```

## Check status

```bash
shki status
```

`status` lists every migration, whether it has been applied, and — when it can
reach the database — whether an applied migration's file has changed since it
ran (a checksum mismatch). See
[Troubleshooting](../../reference/troubleshooting/#checksum-mismatch-on-an-applied-migration)
if one shows up.

## Adopt an existing database

When a project already has a live database that predates `shki`, capture its
shape once as a baseline and commit the artifacts:

```bash
shki bootstrap            # introspect a dev/staging database, write the baseline
```

`bootstrap` only authors files — the initial migration, its Snapshot, the
Directory Schema, and the Journal entry. It never writes to the database. After
this you no longer need the original database; keep evolving the schema with
`generate`/`create`.

Deploying to environments then depends on the target's state:

```bash
# Existing environment (already has the baseline schema):
shki adopt                # validate live shape == baseline, mark baseline applied, apply newer migrations

# Brand-new / empty environment:
shki migrate              # runs the baseline like any other migration, then the rest
```

`adopt` introspects the target, refuses if the live shape drifts from the
committed baseline Snapshot (override with `--force`), records the baseline as
applied *without executing* it, and then applies any newer migrations. Use
`--mark-only` to stop after marking, `--dry-run` to preview, or pass a migration
name to adopt up to a specific point. `adopt` is idempotent — re-running it only
applies what is still pending.

## Roll back during development

```bash
shki down --dry-run
shki down 1
```

Down Migrations are optional and intended for local iteration. They are not a
recommended production rollback strategy.

## Drop a local migration

```bash
shki drop 0003_add_users
```

`drop` removes the selected local migration file, matching Down Migration,
generated Snapshot, and Journal entry. Pending named drops are non-interactive;
dropping an already-applied migration requires confirmation.
