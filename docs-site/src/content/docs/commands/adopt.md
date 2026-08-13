---
title: shki adopt
description: Adopt an existing database at a committed baseline.
---

```bash
shki adopt [NAME] [--mark-only] [--force] [--dry-run] [--schema NAME]
```

Alias: `shki baseline`. Brings an environment that already has the baseline
schema under shki's control without re-running DDL that would fail against
existing objects. It:

1. introspects the target database,
2. compares its shape with the committed baseline Snapshot and refuses on drift,
3. records the baseline as applied **without executing it**,
4. applies any newer pending migrations.

`NAME` defaults to the earliest schema migration; pass one to adopt up to a
specific point. `adopt` is idempotent — re-running it only applies what is still
pending.

## Options

| Flag                       | Purpose                                                                   |
| -------------------------- | --------------------------------------------------------------------------- |
| `--dry-run`                | Show what would be validated, marked, and applied. Changes nothing.        |
| `--mark-only`              | Mark the baseline applied, but stop before applying newer migrations.      |
| `--force`                  | Mark applied even when the live shape differs from the baseline Snapshot.  |
| `--schema <NAME>`          | Schema to introspect for validation (PostgreSQL, default `public`).        |
| `-u, --database-url <URL>` | Target database. Env: `DATABASE_URL`.                                      |

## Examples

```bash
shki adopt --dry-run          # what would be marked and applied
shki adopt                    # mark the baseline, then catch up
shki adopt --mark-only        # mark only; apply later with `shki migrate`
shki adopt 0002_add_orders    # adopt up to a specific migration
```

## When drift is reported

A refusal means the environment is not actually at the baseline: something was
changed outside shki, or the baseline was authored from a different database.
Reconcile the difference, or use `--force` once you're satisfied it's benign —
`--force` marks the migration applied regardless, so shki will assume the
difference away from then on.

Use [`shki migrate`](/shki/commands/migrate/), not `adopt`, on empty databases.

See also: [`shki bootstrap`](/shki/commands/bootstrap/),
[Troubleshooting](/shki/reference/troubleshooting/#adopt-refuses-live-shape-differs-from-the-baseline).
