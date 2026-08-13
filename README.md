<div align="center">
<img src="/assets/shki-logo.png" alt="shki-logo" style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</div>

> [!WARNING]
> `shki` is still a work in progress. Declarative Schema support is active, but some deeper diff/render coverage and validation workflows are still being built.

# shki

`shki` manages database schema change by comparing an intended database shape
with recorded schema history and producing migration artifacts. It is SQL-first
and Drizzle-inspired:

- You author a **Declarative Schema** in SQL.
- `shki` compiles it in a disposable **Shadow Database**.
- The resulting **Snapshot** is compared with the latest recorded Snapshot from the **Journal**.
- `shki diff` previews the **Migration Plan**.
- `shki generate` writes migration SQL, a new Snapshot, and a Journal entry.
- `shki migrate` applies migration artifacts to the live database.

It also generates Rust/TypeScript/Protobuf types from that schema, and
type-safe `sqlx` query functions from annotated SQL.

**📖 Documentation: <https://dk0d.github.io/shki>**

## Install

```bash
# macOS and Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/dk0d/shki/releases/latest/download/shki-installer.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://github.com/dk0d/shki/releases/latest/download/shki-installer.ps1 | iex
```

Also available via `cargo binstall --git https://github.com/dk0d/shki shki` or
`cargo install --git https://github.com/dk0d/shki`. See the
[installation guide](https://dk0d.github.io/shki/getting-started/installation/).

## Quick start

```bash
shki init db --dialect postgres          # scaffold shki.toml, schema/, migrations/
export DATABASE_URL='postgres://user:pass@localhost:5432/mydb'
$EDITOR db/schema/main.sql               # write the schema you want
shki diff                                # preview the Migration Plan
shki generate create_users --down        # write migration SQL + Snapshot + Journal entry
shki migrate                             # apply pending migrations
```

No PostgreSQL install is needed for the Shadow Database — `shki` manages an
embedded one by default. Full walkthrough:
[Quick Start](https://dk0d.github.io/shki/getting-started/quick-start/).

## Supported dialects

| Workflow                                 | PostgreSQL | MySQL | SQLite  |
| ---------------------------------------- | ---------- | ----- | ------- |
| Apply/status/down migration runner       | yes        | yes   | yes     |
| Custom Migration creation                | yes        | yes   | yes     |
| Dump live shape as SQL/JSON              | yes        | yes   | yes     |
| Dump live shape as Directory Schema      | yes        | yes   | yes     |
| Declarative Schema compile/diff/generate | yes        | no    | planned |
| Rich Catalog introspection coverage      | strongest  | basic | basic   |

## Documentation

| Topic                                                                        | What's there                                               |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------- |
| [How it works](https://dk0d.github.io/shki/getting-started/how-it-works/)    | Declarative Schema, Shadow Database, Snapshots, Journal    |
| [Declarative Schema](https://dk0d.github.io/shki/guides/declarative-schema/) | Shadow Database setup, extensions, multi-file schemas      |
| [Migrations](https://dk0d.github.io/shki/guides/migrations/)                 | diff, generate, Custom Migrations, apply, adopt, roll back |
| [Code generation](https://dk0d.github.io/shki/guides/codegen/)               | Rust, TypeScript, and Protobuf types from schema shape     |
| [Typed queries](https://dk0d.github.io/shki/guides/queries/)                 | type-safe `sqlx` functions from annotated SQL              |
| [CLI reference](https://dk0d.github.io/shki/reference/cli/)                  | every command, flag, and alias                             |
| [Configuration](https://dk0d.github.io/shki/reference/configuration/)        | `shki.toml` keys and environment variables                 |

## Contributing

Issues and pull requests are welcome. See
[Contributing](https://dk0d.github.io/shki/contributing/) for the dev setup and
test workflow.

## License

MIT, see [LICENSE](LICENSE).

## Related projects

`shki` stands on the shoulders of projects that explored these ideas first:

- [pgschema](https://github.com/pgschema/pgschema) — declarative, Terraform-style schema management for PostgreSQL.
- [jayy-lmao/sql-gen](https://github.com/jayy-lmao/sql-gen) — generating typed Rust code from a live database schema.
- [squirrel](https://github.com/giacomocavalieri/squirrel) — type-safe SQL code generation from introspected queries.
- [goose](https://github.com/pressly/goose) — a database migration tool. Supports SQL migrations and Go functions.
