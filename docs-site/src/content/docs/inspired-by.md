---
title: Inspired By
description: The projects whose ideas shki builds on.
---

`shki` stands on the shoulders of projects that explored these ideas first.
Thanks to their authors — most of what's good here started as something one of
them proved out.

- **[pgschema](https://github.com/pgschema/pgschema)** — declarative,
  Terraform-style schema management for PostgreSQL. The plan-then-apply shape of
  [`diff`](/shki/commands/diff/) and [`generate`](/shki/commands/generate/) owes
  it a great deal.
- **[Drizzle ORM](https://orm.drizzle.team)** — the SQL-first philosophy and the
  Snapshot + Journal migration history that
  [`shki`](/shki/getting-started/how-it-works/) records alongside each migration.
- **[jayy-lmao/sql-gen](https://github.com/jayy-lmao/sql-gen)** — generating
  typed Rust code from a database schema, the idea behind
  [`codegen`](/shki/commands/codegen/).
- **[squirrel](https://github.com/giacomocavalieri/squirrel)** — type-safe SQL
  code generation from introspected queries, an ancestor of
  [`queries`](/shki/commands/queries/).
- **[sqlc](https://sqlc.dev)** — the `-- name: … :one` annotation style that
  [typed queries](/shki/guides/queries/) borrow wholesale.
- **[goose](https://github.com/pressly/goose)** — a database migration tool
  supporting SQL migrations and Go functions; the migration-runner behavior
  behind [`migrate`](/shki/commands/migrate/) and [`status`](/shki/commands/status/).

Also thanks to [Astro Starlight](https://starlight.astro.build), which this
documentation site is built with.
