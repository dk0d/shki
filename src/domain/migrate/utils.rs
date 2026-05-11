use chrono::Utc;

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
pub fn generate_blank_migration_template(migration_name: &str, is_down: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_sql_normalizes_whitespace_before_truncating() {
        assert_eq!(
            truncate_sql("SELECT\n  *\tFROM users", 64),
            "SELECT * FROM users"
        );
        assert_eq!(
            truncate_sql("SELECT    *    FROM users", 10),
            "SELECT * F..."
        );
    }

    #[test]
    fn sanitize_migration_name_collapses_non_alphanumeric_runs() {
        assert_eq!(
            sanitize_migration_name(" Add  users! table "),
            "add-users-table"
        );
        assert_eq!(sanitize_migration_name("___"), "");
    }

    #[test]
    fn blank_template_contains_basic_header() {
        let template = generate_blank_migration_template("0001_create_users", true);

        assert!(template.starts_with("-- Migration: 0001_create_users (down)\n"));
        assert!(template.contains("-- Created at: "));
    }
}
