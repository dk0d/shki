pub mod mysql;
pub mod pg;
pub mod sqlite;

use crate::SchemaDialect;

fn qualified_table(schema_name: Option<&str>, table_name: &str) -> String {
    schema_name
        .map(|schema| format!("\"{}\".\"{}\"", schema, table_name))
        .unwrap_or_else(|| format!("\"{}\"", table_name))
}

pub fn delete_table(
    dialect: &SchemaDialect,
    schema_name: Option<&str>,
    table_name: &str,
) -> String {
    let table_name = qualified_table(schema_name, table_name);

    match dialect {
        SchemaDialect::Postgres | SchemaDialect::Sqlite => {
            format!("DELETE FROM {} WHERE name = $1", table_name)
        }
        SchemaDialect::Mysql => {
            format!("DELETE FROM {} WHERE name = ?", table_name)
        }
    }
}

pub fn select_migrations(
    dialect: &SchemaDialect,
    schema_name: Option<&str>,
    table_name: &str,
) -> String {
    let table_name = qualified_table(schema_name, table_name);

    match dialect {
        SchemaDialect::Postgres => format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        ),
        SchemaDialect::Mysql => format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        ),

        SchemaDialect::Sqlite => format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        ),
    }
}

pub fn insert_migration(
    dialect: &SchemaDialect,
    schema_name: Option<&str>,
    table_name: &str,
) -> String {
    let table_name = qualified_table(schema_name, table_name);

    match dialect {
        SchemaDialect::Postgres => {
            format!(
                "INSERT INTO {} (name, checksum) VALUES ($1, $2)",
                table_name
            )
        }
        SchemaDialect::Mysql => {
            format!("INSERT INTO {} (name, checksum) VALUES (?, ?)", table_name)
        }
        SchemaDialect::Sqlite => {
            format!("INSERT INTO {} (name, checksum) VALUES (?, ?)", table_name)
        }
    }
}

pub fn ensure_migrations(
    dialect: &SchemaDialect,
    schema_name: Option<&str>,
    table_name: &str,
) -> String {
    let table_name = qualified_table(schema_name, table_name);

    // use text for `applied_at` for simplicity across dialects
    // allows us to use AnyPool more easily
    // checksum is VARCHAR(64) for SHA-256 hex representation
    match dialect {
        SchemaDialect::Postgres => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
        SchemaDialect::Mysql => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
        SchemaDialect::Sqlite => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    checksum TEXT,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
    }
}

/// Add checksum column to existing migrations table
///
/// This is used when upgrading an existing database that has migrations
/// recorded without checksums. The column is nullable to support migrations
/// that were applied before checksum tracking was added.
pub fn alter_migrations_add_checksum(
    dialect: &SchemaDialect,
    schema_name: Option<&str>,
    table_name: &str,
) -> String {
    let table_name = qualified_table(schema_name, table_name);

    match dialect {
        SchemaDialect::Postgres => format!(
            "ALTER TABLE {} ADD COLUMN IF NOT EXISTS checksum VARCHAR(64)",
            table_name
        ),
        SchemaDialect::Mysql => {
            format!("ALTER TABLE {} ADD COLUMN checksum VARCHAR(64)", table_name)
        }
        SchemaDialect::Sqlite => format!("ALTER TABLE {} ADD COLUMN checksum TEXT", table_name),
    }
}
