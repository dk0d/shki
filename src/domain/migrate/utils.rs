use chrono::Utc;

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
    _dialect: SqlDialect,
    is_down: bool,
) -> String {
    let mut content = String::new();

    let direction = if is_down { "down" } else { "up" };
    writeln!(
        &mut content,
        "-- Migration: {} ({})",
        migration_name, direction
    )
    .expect("writing to String cannot fail");
    writeln!(&mut content, "-- Created at: {}", Utc::now().to_rfc3339())
        .expect("writing to String cannot fail");

    content
}
