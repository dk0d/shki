---
title: Troubleshooting
description: Common failures and what they mean.
---

## Checksum mismatch on an applied migration

`status` and `migrate` compare each applied migration's recorded checksum with
the file on disk. A mismatch means the file changed after it was applied —
usually someone edited a migration instead of adding a new one. Restore the
original SQL and write a follow-up migration for the change.

Checksums are computed on comment-stripped SQL, so adding or editing comments
(including `shki:` directives) does not invalidate one.

## A `CREATE INDEX CONCURRENTLY` migration failed

A failed concurrent index build leaves an `INVALID` index behind, and a re-run
guarded by `IF NOT EXISTS` would silently keep it. Check `pg_index.indisvalid`,
`DROP INDEX` the invalid one, then re-run the migration. See
[running outside a transaction](/shki/guides/migrations/#running-outside-a-transaction).

## `generate` fails asking about CONCURRENTLY

When the Declarative Schema declares `CREATE INDEX CONCURRENTLY`,
[`shki generate`](/shki/commands/generate/#concurrent-indexes) writes a second,
no-transaction migration for the index builds — and asks for confirmation
first, since the output changes shape. Declining, or running without a terminal
(CI, scripts), fails the whole generation and writes nothing. Run `generate`
interactively and confirm, or remove `CONCURRENTLY` from the schema to get a
single plain migration.

## Shadow Database refused

Two guards protect against pointing `shki` at a database it may reset:

- `shadow_database_url` must not equal `database_url`.
- An external Shadow Database must be marked as shki-owned:

  ```sql
  COMMENT ON DATABASE shki_shadow IS 'shki:shadow';
  ```

The Shadow Database's user schemas are reset before every compile, so it must be
disposable and dedicated.

## An extension isn't available in the Shadow Database

Embedded PostgreSQL ships without extensions like PostGIS and pgvector. If your
Declarative Schema declares one, point `shadow_database_url` at an external,
shki-owned PostgreSQL image that has it installed.

## `adopt` refuses: live shape differs from the baseline

`adopt` introspects the target and compares it with the committed baseline
Snapshot. Drift means the environment isn't actually at the baseline. Either
reconcile the database, or override with `--force` once you're satisfied the
difference is benign. `--dry-run` shows what it would validate and apply.

## A `shki:` directive is rejected

Unrecognized `shki:` directives are hard errors, not silent no-ops, so a typo
surfaces before it reaches production. The supported directive is
`shki:no-transaction`.

## Query codegen fails on a type

Types the Rust generator renders as `String` but sqlx cannot decode as `String`
(`NUMERIC`, ranges, network, geometric, and interval types) need a
`[codegen.type_overrides]` entry mapping them to a compatible Rust type.
