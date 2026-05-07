use chrono::Utc;

use crate::MIGRATION_SPLIT_MARKER;
use crate::schema::SqlDialect;
use std::fmt::Write as _;

/// Truncate a SQL statement for display in error messages
pub fn truncate_sql(sql: &str, max_len: usize) -> String {
    let mut normalized = String::with_capacity(sql.len());
    for token in sql.split_whitespace() {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(token);
    }

    if normalized.len() <= max_len {
        normalized
    } else {
        format!("{}...", &normalized[..max_len])
    }
}

/// Sanitize a migration name to be filesystem-safe
pub fn sanitize_migration_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_dash = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

/// Generate a blank migration template with helpful comments
///
/// # Arguments
/// * `migration_name` - The name of the migration
/// * `dialect` - The database dialect
/// * `is_down` - Whether this is a down migration template
pub fn generate_blank_migration_template(
    migration_name: &str,
    dialect: SqlDialect,
    is_down: bool,
) -> String {
    let mut content = String::new();
    let marker_line = format!("-- {}\n", MIGRATION_SPLIT_MARKER);

    let direction = if is_down { "down" } else { "up" };
    writeln!(
        &mut content,
        "-- Migration: {} ({})",
        migration_name, direction
    )
    .expect("writing to String cannot fail");
    writeln!(&mut content, "-- Created at: {}", Utc::now().to_rfc3339())
        .expect("writing to String cannot fail");
    content.push_str("-- Type: manual\n");
    content.push_str("--\n");

    if is_down {
        content.push_str("-- This migration reverses the changes made by the up migration.\n");
    } else {
        content.push_str("-- This migration was created for manual editing.\n");
    }

    content.push_str("-- Write your SQL statements below.\n");
    content.push_str("--\n");
    content.push_str("-- Tips:\n");
    content.push_str("-- - The entire migration runs in a single transaction\n");
    content.push_str("-- - If any statement fails, all changes are rolled back\n");
    writeln!(
        &mut content,
        "-- - Use '{}' to visually separate statements",
        MIGRATION_SPLIT_MARKER
    )
    .expect("writing to String cannot fail");
    content.push_str("-- - Remove these comments before committing\n");
    content.push_str("--\n");

    // Add dialect-specific examples
    if is_down {
        match dialect {
            SqlDialect::Postgres => {
                content.push_str("-- Example PostgreSQL rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX CONCURRENTLY IF EXISTS idx_users_email;\n");
                content.push_str(&marker_line);
                content.push_str("-- ALTER TABLE posts DROP COLUMN IF EXISTS view_count;\n");
                content.push_str(&marker_line);
                content.push_str("-- DROP TYPE IF EXISTS status_type;\n");
            }
            SqlDialect::Mysql => {
                content.push_str("-- Example MySQL rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX idx_users_email ON users;\n");
                content.push_str(&marker_line);
                content.push_str("-- ALTER TABLE posts DROP COLUMN view_count;\n");
            }
            SqlDialect::Sqlite => {
                content.push_str("-- Example SQLite rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX IF EXISTS idx_users_email;\n");
                content.push_str(&marker_line);
                content.push_str("-- Note: SQLite doesn't support DROP COLUMN directly.\n");
                content.push_str("-- You may need to recreate the table without the column.\n");
            }
        }
    } else {
        match dialect {
            SqlDialect::Postgres => {
                content.push_str("-- Example PostgreSQL statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX CONCURRENTLY idx_users_email ON users(email);\n");
                content.push_str(&marker_line);
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INTEGER DEFAULT 0;\n");
                content.push_str(&marker_line);
                content.push_str("-- CREATE TYPE status_type AS ENUM ('active', 'inactive');\n");
            }
            SqlDialect::Mysql => {
                content.push_str("-- Example MySQL statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX idx_users_email ON users(email);\n");
                content.push_str(&marker_line);
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INT DEFAULT 0;\n");
            }
            SqlDialect::Sqlite => {
                content.push_str("-- Example SQLite statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX idx_users_email ON users(email);\n");
                content.push_str(&marker_line);
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INTEGER DEFAULT 0;\n");
            }
        }
    }

    content.push_str("\n\n-- Write your SQL below this line:\n\n");

    content
}
