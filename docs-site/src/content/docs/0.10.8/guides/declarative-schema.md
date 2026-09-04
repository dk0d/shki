---
title: Declarative Schema
description: Author schema in SQL, compile it in a Shadow Database, and split it
  across files.
slug: 0.10.8/guides/declarative-schema
---

A Declarative Schema is the intended shape of your database, written in SQL.
`shki` compiles it in a disposable Shadow Database, snapshots the result, and
diffs that Snapshot against the latest one recorded in the Journal.

## Shadow Database configuration

Declarative Schema compilation requires PostgreSQL execution in a Shadow Database.

By default, `shki` uses managed embedded PostgreSQL. You can pin the embedded
PostgreSQL major version on commands that compile a Declarative Schema:

```bash
shki generate create_users --pg-version 16
```

Supported embedded major versions are `14`, `15`, `16`, `17`, and `18`; the
default is `18`. Match it to the version you run in production so the compiled
shape reflects the same server behavior.

For CI, locked-down environments, or teams that want explicit provisioning,
configure an external Shadow Database:

```bash
export SHKI_SHADOW_DATABASE_URL='postgres://user:pass@localhost:5432/shki_shadow'
```

The Shadow Database is disposable. `shki` resets user schemas before applying the
Declarative Schema. `shadow_database_url` must not be the same as `database_url`,
and an external shadow must be marked as Shki-owned:

```sql
COMMENT ON DATABASE shki_shadow IS 'shki:shadow';
```

:::caution
Always use a disposable Shadow Database. `shki` resets user schemas in the
configured Shadow Database before compilation.
:::

## PostgreSQL extensions

Declare extensions in the schema before objects that use their types. Shki tracks
the extension and preserves extension-defined column types as custom types.

```sql
CREATE EXTENSION postgis;
CREATE TABLE places (location geometry(Point, 4326) NOT NULL);

CREATE EXTENSION vector;
CREATE TABLE embeddings (embedding vector(3) NOT NULL);
```

The Shadow Database image must have each declared extension installed. Embedded
PostgreSQL does not provide PostGIS or pgvector; configure an external,
Shki-owned PostgreSQL image that includes them. Extension type modifiers, such
as `vector(3)`, `halfvec(384)`, and `geometry(Point, 4326)`, are retained in
Snapshots and detected by schema diffs.

## Directory Schemas

A Declarative Schema can be a single SQL file or a directory with a canonical
`main.sql` entrypoint.

```text
schema/
  main.sql
  tables/
    users.sql
```

```sql
-- schema/main.sql
\i tables/users.sql
```

Only `\i` include directives are supported in v1. Include paths are resolved
relative to the including file, and include cycles are rejected.
