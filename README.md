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

- **Declarative Schema Definition**: Define tables, columns, indexes, and constraints in Lua or Rust
  - type completions 
- **Automatic Migration Generation**: Diff your schema against the database and generate migrations
- **Multi-Dialect Support**: 
    - [ ] PostgreSQL
    - [ ] MySQL
    - [ ] SQLite
- **Database Introspection**: Pull existing database schemas into Rust code
- **Type-Safe**: Leverage Rust's type system for schema definitions
- **Code Gen** : Generate Rust structs/enums to work with DB.
    - [ ] support custom derives etc. to support sqlx, Diesel, SeaORM.

## Use Patterns

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
├── shki.yml       # selene standard lib support
└── shki.toml      # shki project configuration
```

The main entry point of the schema is `init.lua`.


### Rust Lib

Write your schema as a Rust binary...

...more coming soon
