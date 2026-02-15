<p align="center">
<img src="/assets/shki-logo.png" alt='shki-logo' style="width: 50%; border-radius: 0.5rem; filter: drop-shadow(0 4px);"/>
</p>

> [!WARNING]
> This is currently a work-in-progress and planned features may not be fully finished or entirely missing

# shki

A drizzle-orm inspired database schema management and migration tool using Lua and Rust.

**shki** allows you to define your database schema declaratively in Lua, then automatically generates migrations by diffing your schema definition against the current database state — no more writing migrations by hand.

## Why shki?

| Feature            | Status                            | shki                           | Traditional Migration Tools |
| -                  | -                                 | -                              | -                           |
| Schema definition  | ✅                                | Declarative (Lua/Rust)         | Imperative SQL files        |
| Migration creation | ✅                                | Auto-generated from diff       | Manual writing              |
| Type safety        | ✅                                | Full IDE autocomplete          | None                        |
| Code generation    | ✅                                | Rust structs from schema       | Usually separate tool       |
| Rollback support   | ✅ [⚠️](#note-on-down-migrations) | Auto-generated down migrations | Manual writing              |


## Installation

### From source

```bash
cargo install --git https://github.com/dk0d/shki
```

### Build locally

```bash
git clone https://github.com/dk0d/shki
cd shki
cargo build --release # Binary will be at ./target/release/shki
```

or 

```bash
cargo install --path .
```

## Quick Start

```bash
# Initialize a new shki project with Lua schema support
shki init my_db

# Edit db/init.lua to define your schema, then generate migrations
shki generate

# Apply migrations to your database
shki migrate
```

## Features

- **Declarative Schema Definition**: Define tables, columns, indexes, and constraints
  - [x] Lua type completions via Language Server
  - [ ] Rust-based binary project
- **Automatic Migration Generation**: Diff your schema against the database and generate migrations
- **Multi-Dialect Support**: 
  - [x] PostgreSQL
  - [ ] MySQL
  - [ ] SQLite
- **Database Introspection**: Pull existing database schemas into code
- **Code Generation**: Generate Rust structs/enums from your schema
  - [ ] Support custom derives for sqlx, Diesel, SeaORM

### Supported Schema Elements

| Element            | PostgreSQL | MySQL | SQLite |
| -                  | -          | -     | -      |
| Tables             | ✅         | 🚧    | 🚧     |
| Columns            | ✅         | 🚧    | 🚧     |
| Primary Keys       | ✅         | 🚧    | 🚧     |
| Foreign Keys       | ✅         | 🚧    | 🚧     |
| Unique Constraints | ✅         | 🚧    | 🚧     |
| Check Constraints  | ✅         | 🚧    | 🚧     |
| Indexes            | ✅         | 🚧    | 🚧     |
| Partial Indexes    | ✅         | —     | 🚧     |
| Enums              | ✅         | —     | —      |
| Views              | ✅         | 🚧    | 🚧     |
| Sequences          | ✅         | —     | —      |
| Extensions         | ✅         | —     | —      |
| Comments           | ✅         | 🚧    | —      |

## CLI Commands

```
shki <command> [options]

Commands:
  init      Initialize a new shki project
  generate  Generate migrations from schema changes (alias: gen)
  migrate   Apply pending migrations (alias: up)
  down      Rollback applied migrations
  create    Create a blank migration file (alias: new)
  pull      Introspect database schema
  diff      Show diff between schema and database
  status    List migrations and their status (alias: s)
  codegen   Generate Rust structs from schema (alias: code)
  drop      Drop a migration file

Global Options:
  -c, --config <PATH>      Path to config file [default: shki.toml]
  -l, --dialect <DIALECT>  Database dialect (pg, mysql, sqlite)
  -u, --database-url <URL> Database connection URL
  -o, --out <PATH>         Output directory for migrations
  -v, --verbose            Verbose output
```

## Usage Patterns

### 1. Pure Migration Runner (SQL files only)

If you prefer writing SQL migrations manually:

```bash
# Create a minimal config
shki init --simple

# Create empty migration files
shki create add_users_table --with-down

# Edit the SQL files manually, then apply
shki migrate

# Check status
shki status

# Rollback if needed
shki down
```

### 2. Lua Schema + Auto-Generated Migrations (Recommended)

Define your schema declaratively and let shki generate migrations:

```bash
# Initialize a full Lua project
shki init db
```

This creates:

```
db/
├── migrations/        # Generated migrations live here
│   └── _meta/
├── lua/               # Additional Lua modules
├── .luacats/          # Type definitions for IDE support
├── .luarc.json        # Lua Language Server config
├── selene.toml        # Selene linter config
├── init.lua           # Main schema entry point
└── shki.toml          # Project configuration
```

#### Defining Your Schema

Edit `init.lua`:

```lua
local schema = pg.schema("public")
local E = EnumBuilder
local C = ColumnBuilder
local T = TableBuilder

-- Define an enum type
schema:enum_type(
    E.new("user_status")
        :description("User account status")
        :value("active")
        :value("inactive")
        :value("suspended")
)

-- Define the users table
schema:table(
    T.new("users")
        :description("User accounts")
        :column(C.uuid("id"):primary_key():default_uuidv7())
        :column(C.text("email"):not_null():unique())
        :column(C.text("name"):not_null())
        :column(C.text("password_hash"):not_null())
        :column(C.enum_type("status", "user_status"):not_null():default_value("active"))
        :column(C.timestamptz("created_at"):default_now())
        :column(C.timestamptz("updated_at"):default_now())
        :index(IndexBuilder.new("users_email_idx"):column("email"):unique())
)

-- Define the posts table with a foreign key
schema:table(
    T.new("posts")
        :description("User blog posts")
        :column(C.uuid("id"):primary_key():default_uuidv7())
        :column(C.uuid("author_id"):not_null():references("users", "id", "CASCADE"))
        :column(C.text("title"):not_null())
        :column(C.text("content"))
        :column(C.boolean("published"):not_null():default_value(false))
        :column(C.timestamptz("created_at"):default_now())
)

return schema
```

#### Organizing Larger Schemas

Split tables into separate files in the `lua/` directory:

```lua
-- lua/posts.lua
local M = {}

M.posts = TableBuilder.new("posts")
    :description("Blog posts")
    :column(ColumnBuilder.uuid("id"):primary_key():default_uuidv7())
    :column(ColumnBuilder.uuid("author_id"):not_null():references("users", "id", "CASCADE"))
    :column(ColumnBuilder.text("title"):not_null())
    :column(ColumnBuilder.text("content"))
    :column(ColumnBuilder.timestamptz("created_at"):default_now())

return M
```

Then import in `init.lua`:

```lua
schema:table(require("posts").posts)
```

#### Generate and Apply Migrations

```bash
# See what would change
shki diff

# Generate migration files
shki generate --name add_posts_table

# Preview without applying
shki migrate --dry-run

# Apply migrations
shki migrate
```

### 3. Working with Existing Databases

Pull an existing database schema:

```bash
# Output as SQL
shki pull --format sql

# Output as JSON (useful for inspection)
shki pull --format json --output schema.json
```

### 4. Generate Rust Code

Generate type-safe Rust structs from your schema:

```bash
# Generate as a module (multiple files)
shki codegen --out src/models --mode module

# Generate as a single file
shki codegen --out src/models.rs --mode single
```

## Configuration

Configuration can be set via `shki.toml` or environment variables prefixed with `SHKI_`.

### shki.toml

```toml
# Database connection (can be overridden by SHKI_DATABASE_URL)
database_url = "postgres://user:pass@localhost:5432/mydb"

# Migration settings
[migrations]
table = "__shki_migrations"    # Table to track applied migrations
prefix = "timestamp"           # "timestamp" or "sequential"
generate_down = true           # Auto-generate rollback migrations

# Schema settings  
[schema]
path = "init.lua"              # Entry point for Lua schema
```
#### Note on Down Migrations

⚠️ - Down migrations are mostly intended for local development and fast iteration - but I don't recommend them in production.

- **Data Loss**: Dropping a column or table via a down migration often means permanently losing data that was inserted after the forward migration was deployed.
- **Incompatibility with modern CI/CD**: Modern systems move forward, not backward. 

References: 
- [Why you will never write another "down" migration](https://antman-does-software.com/why-you-will-never-write-another-down-migration)
- [The Myth of Down Migrations](https://atlasgo.io/blog/2024/04/01/migrate-down)

**Key advantages:**
- **Declarative schemas**: Define *what* your schema should look like, not *how* to get there
- **Automatic migrations**: Schema changes are detected and migrations are generated automatically
- **IDE support**: Full autocomplete and type checking in your schema files via Lua Language Server
- **Database introspection**: Pull existing database schemas into code
- **Rust codegen**: Generate type-safe Rust structs directly from your schema

> [!WARNING]
> When using auto-generated down migrations - some statements are not supported by the dialect
> or are not easily calculated (such as altering a sequence) and will require you to manually add
> those statements.
> | Statement     | Reason                                                              |
> | -----------   | --------                                                            |
> | AddEnumValue  | PostgreSQL dialect limitation - no support for removing enum values |
> | AlterSequence | Complex - would need prev values for each change type               |
> | AlterColumn   | Complex - would need prev values for each change type               |

### Environment Variables

Use `__` as a separator for nested properties:

```bash
SHKI_DATABASE_URL="postgres://user:pass@localhost:5432/mydb"
SHKI_MIGRATIONS__TABLE="__shki_migrations"
SHKI_MIGRATIONS__PREFIX="timestamp"
SHKI_MIGRATIONS__GENERATE_DOWN=true
```

shki automatically reads the `.env` file in the current working directory.

## How It Works

shki uses a **snapshot-based diffing** approach:

1. **Schema Definition**: Your Lua code defines the desired state of your database
2. **Database Introspection**: shki queries your database to get its current state
3. **Diff Calculation**: The two states are compared to produce a list of changes
4. **Migration Generation**: Changes are converted to SQL statements (both up and down)
5. **Migration Tracking**: Applied migrations are recorded in a tracking table

This approach means you never write migrations manually — you just update your schema definition and shki figures out how to get there.

## Supported Data Types

### PostgreSQL

| Category   | Types                                                                                                             |
| ---------- | -------                                                                                                           |
| Numeric    | `smallint`, `integer`, `bigint`, `serial`, `bigserial`, `real`, `double precision`, `numeric`, `decimal`, `money` |
| Character  | `char`, `varchar`, `text`, `citext`                                                                               |
| Binary     | `bytea`                                                                                                           |
| Boolean    | `boolean`                                                                                                         |
| Date/Time  | `date`, `time`, `timetz`, `timestamp`, `timestamptz`, `interval`                                                  |
| UUID       | `uuid`                                                                                                            |
| JSON       | `json`, `jsonb`                                                                                                   |
| Network    | `inet`, `cidr`, `macaddr`, `macaddr8`                                                                             |
| Geometric  | `point`, `line`, `lseg`, `box`, `path`, `polygon`, `circle`                                                       |
| Range      | `int4range`, `int8range`, `numrange`, `tsrange`, `tstzrange`, `daterange`                                         |
| Arrays     | Any type as array (e.g., `text[]`, `integer[]`)                                                                   |

## Roadmap

- [x] Generate down migration from diff
- [ ] MySQL dialect support
- [ ] SQLite dialect support
- [ ] Rust-native schema definitions
- [ ] Custom derive support for codegen (sqlx, Diesel, SeaORM)
- [ ] Schema validation and linting
- [ ] Migration squashing

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

MIT License - see [LICENSE](LICENSE) for details.
