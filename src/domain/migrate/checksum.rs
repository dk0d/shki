//! Checksum calculation for migration files
//!
//! Calculates SHA-256 checksums of SQL content while ignoring comments and empty lines.
//! This allows migrations to be annotated with comments without affecting the checksum.

use sha2::{Digest, Sha256};

/// Calculate SHA-256 checksum of SQL content, ignoring comments and empty lines
///
/// The checksum is computed on normalized SQL content:
/// - Single-line comments (`--` and `#`) are removed
/// - Block comments (`/* ... */`) are removed
/// - Empty lines are removed
/// - Leading/trailing whitespace is trimmed from each line
/// - Line endings are normalized to `\n`
pub fn sql_checksum(sql: &str) -> String {
    let normalized = normalize_sql(sql);
    let hash = Sha256::digest(normalized.as_bytes());
    hex_encode(&hash)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

/// Normalize SQL content by removing comments and empty lines
fn normalize_sql(sql: &str) -> String {
    let without_block_comments = strip_block_comments(sql);

    without_block_comments
        .lines()
        .map(|line| strip_line_comment(line).trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip block comments (/* ... */) from SQL, handling nested comments
fn strip_block_comments(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut depth = 0;

    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            depth += 1;
        } else if c == '*' && chars.peek() == Some(&'/') && depth > 0 {
            chars.next(); // consume '/'
            depth -= 1;
        } else if depth == 0 {
            result.push(c);
        }
    }

    result
}

/// Strip single-line comment from a line (-- or #)
fn strip_line_comment(line: &str) -> &str {
    // Handle -- comments
    if let Some(pos) = line.find("--") {
        return &line[..pos];
    }

    // Handle # comments
    if let Some(pos) = line.find('#') {
        return &line[..pos];
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    #[test]
    fn test_normalize_removes_single_line_comments() {
        let sql = "SELECT * FROM users; -- get all users";
        assert_eq!(normalize_sql(sql), "SELECT * FROM users;");
    }

    #[test]
    fn test_normalize_removes_hash_comments() {
        let sql = "SELECT * FROM users; # get all users";
        assert_eq!(normalize_sql(sql), "SELECT * FROM users;");
    }

    #[test]
    fn test_normalize_removes_full_line_comments() {
        let sql = "-- This is a comment\nSELECT * FROM users;";
        assert_eq!(normalize_sql(sql), "SELECT * FROM users;");
    }

    #[test]
    fn test_normalize_removes_block_comments() {
        let sql = "SELECT /* columns */ * FROM users;";
        assert_eq!(normalize_sql(sql), "SELECT  * FROM users;");
    }

    #[test]
    fn test_normalize_removes_multiline_block_comments() {
        let sql = "SELECT *\n/* This is a\nmulti-line\ncomment */\nFROM users;";
        assert_eq!(normalize_sql(sql), "SELECT *\nFROM users;");
    }

    #[test]
    fn test_normalize_removes_empty_lines() {
        let sql = "SELECT *\n\n\nFROM users;";
        assert_eq!(normalize_sql(sql), "SELECT *\nFROM users;");
    }

    #[test]
    fn test_normalize_trims_whitespace() {
        let sql = "  SELECT *  \n  FROM users;  ";
        assert_eq!(normalize_sql(sql), "SELECT *\nFROM users;");
    }

    #[test]
    fn test_normalize_handles_mixed_comments() {
        let sql = r#"
# Another Sql comment
-- Migration: add users table
/* 
 * Creates the users table
 * with basic fields
 */
CREATE TABLE users (
    id SERIAL PRIMARY KEY, -- auto-increment
    name TEXT NOT NULL # required field
);
"#;
        let expected = "CREATE TABLE users (\nid SERIAL PRIMARY KEY,\nname TEXT NOT NULL\n);";
        assert_eq!(normalize_sql(sql), expected);
    }

    #[test]
    fn test_checksum_hello_world() {
        let hash = Sha256::digest(b"hello world");
        assert_eq!(
            hash,
            hex!("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
        let hash = sql_checksum("hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_checksum_is_consistent() {
        let sql = "SELECT * FROM users;";
        let checksum1 = sql_checksum(sql);
        let checksum2 = sql_checksum(sql);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_checksum_ignores_comments() {
        let sql1 = "SELECT * FROM users;";
        let sql2 = "-- Comment\nSELECT * FROM users; -- another comment";
        assert_eq!(sql_checksum(sql1), sql_checksum(sql2));
    }

    #[test]
    fn test_checksum_ignores_whitespace_differences() {
        let sql1 = "SELECT * FROM users;";
        let sql2 = "  SELECT * FROM users;  \n\n";
        assert_eq!(sql_checksum(sql1), sql_checksum(sql2));
    }

    #[test]
    fn test_checksum_differs_for_different_sql() {
        let sql1 = "SELECT * FROM users;";
        let sql2 = "SELECT * FROM accounts;";
        assert_ne!(sql_checksum(sql1), sql_checksum(sql2));
    }

    #[test]
    fn test_checksum_is_64_chars_hex() {
        let checksum = sql_checksum("SELECT 1;");
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_normalize_handles_nested_block_comments() {
        let sql = "SELECT /* outer /* inner */ still outer */ * FROM users;";
        assert_eq!(normalize_sql(sql), "SELECT  * FROM users;");
    }

    #[test]
    fn test_normalize_empty_input() {
        assert_eq!(normalize_sql(""), "");
        assert_eq!(normalize_sql("   "), "");
        assert_eq!(normalize_sql("-- just a comment"), "");
    }
}
