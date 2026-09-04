---
title: Quick Start
description: Initialize a shki project and generate your first migration from a Declarative Schema.
---

## 1. Initialize a project

```bash
shki init db --dialect postgres
```

`init` creates a project layout like:

```text
db/
  shki.toml
  postgres-language-server.jsonc
  schema/
    main.sql
  migrations/
    _meta/
```

For PostgreSQL projects, `postgres-language-server.jsonc` is generated from the
same init defaults so editor tooling points at the Declarative Schema entrypoint.

## 2. Configure your live database URL

```bash
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
```

## 3. Edit the Declarative Schema entrypoint

```sql
-- db/schema/main.sql
CREATE TABLE users (
  id integer PRIMARY KEY,
  email text NOT NULL UNIQUE
);
```

## 4. Preview the Migration Plan

```bash
shki diff
```

## 5. Generate migration artifacts

```bash
shki generate create_users --down
```

This writes the migration SQL, its Snapshot, and a Journal entry (plus a Down
Migration, thanks to `--down`):

```text
db/migrations/
  0000_create_users.sql
  0000_create_users.down.sql
  _meta/
    0000_create_users.snapshot.json
    _journal.json
```

Commit all of it — the Snapshot is the baseline the next `diff` compares against.

## 6. Apply pending migrations

```bash
shki migrate            # everything pending
shki migrate --dry      # or preview first
```

`migrate` runs each pending file and records it, with its checksum, in the
migrations table. Check where things stand at any point:

```bash
shki status
```

## From here

Steps 3–6 are the loop: edit the schema, `diff`, `generate`, `migrate`.

- [How It Works](../../getting-started/how-it-works/) — what the Shadow Database, Snapshots, and Journal are doing
- [Declarative Schema](../../guides/declarative-schema/) — extensions, multi-file schemas, external Shadow Database
- [Migrations](../../guides/migrations/) — hand-written SQL, rollback, adopting an existing database
- Already have a database? [Adopt an existing database](../../guides/migrations/#adopt-an-existing-database) instead of starting from `init`.
