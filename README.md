<p align="center">
<img src="/assets/shki-logo.png" alt='shki-logo' style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</p>

> [!WARNING]
> This is currently a work-in-progress and planned features may not be fully finished or entirely missing

# shki

A drizzle-orm inspired database schema management and migration tool using Lua and Rust.

`shki` allows you to define your database schema declaratively in Lua (or Rust) code,
then automatically generate migrations to transition from the current database
state to your desired schema state.

## Features

- **Declarative Schema Definition**: Define tables, columns, indexes, and constraints
  - [x] Lua type completions 
  - [ ] Rust-based binary project
- **Automatic Migration Generation**: Diff your schema against the database and generate migrations
- **Multi-Dialect Support**: 
    - [x] PostgreSQL
    - [ ] MySQL
    - [ ] SQLite
- **Database Introspection**: Pull existing database schemas into Rust code
- **Type-Safe**: Leverage Rust's type system for schema definitions
- **Code Gen** : Generate Rust structs/enums to work with DB.
    - [ ] support custom derives etc. to support sqlx, Diesel, SeaORM.

## Use Patterns

### Config

Configuration options can be set via `shki.toml` or via environment variables prefixed with `SHKI_`.

For nested properties, use `__` as a separator ( `.`).

For example,

```toml
[migrations]
table = "__shki_migarations"
prefix = "timestamp"
generate_down = true
```

or 

```toml
migrations.table = "__shki_migarations"
migrations.prefix = "timestamp"
migrations.generate_down = true

```

is equivalent to 

```bash
SHKI_MIGRATIONS__TABLE="__shki_migarations"
SHKI_MIGRATIONS__PREFIX="timestamp"
SHKI_MIGRATIONS__GENERATE_DOWN=true
```


`shki` will read `.env` files in the current working directory and looks for 
`SHKI_DATABASE_URL` to connect to your database instance.

You can define a default `database_url` in `shki.toml` for local dev and then override
that value via environment variables.


### Pure migration runner

Create a default config file

```bash
shki init --simple
```

Create empty up and down migration files

```bash
shki create --with-down init # or `shki new`
```

Edit your sql files manually and run migrations

```bash
shki migrate # or `shki up`
```

Rollback

```bash
shki down
```

Status

```bash 
shki status
```

### Lua + CLI

1. Create a `shki` db project in your repo

```bash
shki init db -l lua 
```

This creates a new project with the below structure

```bash
db
├── migrations     # where your migrations live
│   └── _meta
├── lua            # your lua schema def files
│   └── *.lua      # supporting lua files
├── .luacats       # lua language server bindings for autocomplete
├── .luarc.json    # lua language server config
├── .selene.toml   # selene linter config
├── init.lua       # main entry to schema
├── shki.yml       # lua standard lib definition for shki
└── shki.toml      # shki project configuration
```

The main entry point of the schema is `init.lua`.

Example schema

```lua

local schema = pg.schema("public")
local T = TableBuilder
local C = ColumnBuilder

schema.table(
    T::new("users")
    :column(C.new("id").primary_key().uuid().default(DefaultValue::uuidv7()))
)
```


### Rust Lib

Write your schema as a Rust binary...

...more coming soon
