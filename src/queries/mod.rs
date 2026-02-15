pub mod mysql;
pub mod pg;
pub mod sqlite;

use crate::SchemaDialect;

pub fn delete_table(
    dialect: &SchemaDialect,
    schema_name: &Option<String>,
    table_name: &String,
) -> String {
    // Build the SQL for removing the migration record
    let table_name = match &schema_name {
        Some(s) => format!("\"{}\".\"{}\"", s, table_name),
        None => format!("\"{}\"", table_name),
    };

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
    schema_name: &Option<String>,
    table_name: &String,
) -> String {
    let table_name = match &schema_name {
        Some(s) => format!("\"{}\".\"{}\"", s, table_name),
        None => format!("\"{}\"", table_name),
    };

    match dialect {
        SchemaDialect::Postgres => format!(
            "SELECT id, name, applied_at from {} ORDER BY id",
            table_name
        ),
        SchemaDialect::Mysql => format!(
            "SELECT id, name, applied_at from {} ORDER BY id",
            table_name
        ),

        SchemaDialect::Sqlite => format!(
            "SELECT id, name, applied_at from {} ORDER BY id",
            table_name
        ),
    }
}

pub fn insert_migration(
    dialect: &SchemaDialect,
    schema_name: &Option<String>,
    table_name: &String,
) -> String {
    let table_name = match &schema_name {
        Some(s) => format!("\"{}\".\"{}\"", s, table_name),
        None => format!("\"{}\"", table_name),
    };

    match dialect {
        SchemaDialect::Postgres => {
            format!("INSERT INTO {} (name) VALUES ($1)", table_name)
        }
        SchemaDialect::Mysql => {
            format!("INSERT INTO {} (name) VALUES (?)", table_name)
        }
        SchemaDialect::Sqlite => {
            format!("INSERT INTO {} (name) VALUES (?)", table_name)
        }
    }
}

pub fn ensure_migrations(
    dialect: &SchemaDialect,
    schema_name: &Option<String>,
    table_name: &String,
) -> String {
    let table_name = match &schema_name {
        Some(s) => format!("\"{}\".\"{}\"", s, table_name),
        None => format!("\"{}\"", table_name),
    };

    // use text for `applied_at` for simplicity across dialects
    // allows us to use AnyPool more easily
    match dialect {
        SchemaDialect::Postgres => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
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
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
    }
}
