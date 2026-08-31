//! Parsing of annotated query files.
//!
//! Query files are plain `*.sql` files containing one or more named queries,
//! each introduced by an sqlc-style annotation:
//!
//! ```sql
//! -- name: active_users :many
//! SELECT id, email FROM users WHERE active = true;
//! ```
//!
//! The annotation `name:` becomes the generated function name verbatim (cased
//! per target language) and the `:one` / `:many` / `:exec` tag selects the
//! result cardinality. See `docs/adr/0001-typed-query-codegen.md`.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::{Result, ShkiError};

/// How many rows a query is expected to return, and therefore the shape of the
/// generated function's return type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// At most one row: `Result<Option<Row>>`.
    One,
    /// Zero or more rows: `Result<Vec<Row>>`.
    Many,
    /// No rows; returns the affected row count: `Result<u64>`.
    Exec,
    /// A paginated `:many`: `Result<Page<Row>>`. See the query codegen ADR.
    Batch,
}

/// A keyset bind parameter and the selected result field that supplies its next
/// cursor value, e.g. `$1=id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetParam {
    pub parameter: String,
    pub field: String,
}

impl Cardinality {
    fn parse(tag: &str) -> Option<Self> {
        match tag {
            "one" => Some(Self::One),
            "many" => Some(Self::Many),
            "exec" => Some(Self::Exec),
            "batch" => Some(Self::Batch),
            _ => None,
        }
    }
}

/// A single named query parsed out of a query file.
#[derive(Debug, Clone)]
pub struct QuerySpec {
    /// The author-provided name; becomes the function name.
    pub name: String,
    pub cardinality: Cardinality,
    /// Keyset cursor parameter/result-field mappings from a `:keyset` modifier
    /// (e.g. `$1=id $2=created_at`), in cursor-key order.
    pub keyset: Vec<KeysetParam>,
    /// Whether the generated function requires a transaction executor (`:tx`).
    pub transaction: bool,
    /// The SQL body, with the annotation line stripped and trimmed.
    pub sql: String,
    /// The file this query came from (for diagnostics).
    pub source_file: PathBuf,
}

/// Parse every `*.sql` file in `dir` into a flat, file-then-declaration-ordered
/// list of query specs.
pub fn parse_query_dir(dir: &Path) -> Result<Vec<QuerySpec>> {
    if !dir.exists() {
        return Err(ShkiError::config(format!(
            "Query path not found: {}",
            dir.display()
        )));
    }

    let mut files: Vec<PathBuf> = if dir.is_dir() {
        std::fs::read_dir(dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect()
    } else {
        vec![dir.to_path_buf()]
    };
    files.sort();

    let mut specs = Vec::new();
    for file in files {
        let content = std::fs::read_to_string(&file)?;
        specs.extend(parse_query_file(&content, &file)?);
    }

    Ok(specs)
}

/// Parse a single query file's contents.
pub fn parse_query_file(content: &str, source_file: &Path) -> Result<Vec<QuerySpec>> {
    // e.g. `-- name: active_users :many` or `-- name: page :batch :keyset $1=id $2=created_at`
    let marker =
        Regex::new(r"^\s*--\s*name:\s*(?P<name>\S+)\s+(?P<rest>:.*?)\s*$").expect("valid regex");

    let mut specs: Vec<QuerySpec> = Vec::new();
    let mut current: Option<(String, Cardinality, Vec<KeysetParam>, bool)> = None;
    let mut body: Vec<&str> = Vec::new();

    let flush = |current: &mut Option<(String, Cardinality, Vec<KeysetParam>, bool)>,
                 body: &mut Vec<&str>,
                 specs: &mut Vec<QuerySpec>| {
        if let Some((name, cardinality, keyset, transaction)) = current.take() {
            // Trailing comment/blank lines after the statement annotate the
            // NEXT query (doc comments, test expectations), not this SQL.
            while body.last().is_some_and(|line| {
                let trimmed = line.trim();
                trimmed.is_empty() || trimmed.starts_with("--")
            }) {
                body.pop();
            }
            let sql = body
                .join("\n")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string();
            specs.push(QuerySpec {
                name,
                cardinality,
                keyset,
                transaction,
                sql,
                source_file: source_file.to_path_buf(),
            });
        }
        body.clear();
    };

    for line in content.lines() {
        if let Some(caps) = marker.captures(line) {
            // A new query begins; finish the previous one.
            flush(&mut current, &mut body, &mut specs);

            let name = caps["name"].to_string();
            let (cardinality, keyset, transaction) =
                parse_directives(&caps["rest"], &name, source_file)?;
            current = Some((name, cardinality, keyset, transaction));
        } else if current.is_some() {
            body.push(line);
        }
        // Lines before the first marker are ignored (file-level comments).
    }
    flush(&mut current, &mut body, &mut specs);

    for spec in &specs {
        if spec.sql.is_empty() {
            return Err(ShkiError::config(format!(
                "Query '{}' in {} has no SQL body",
                spec.name,
                spec.source_file.display()
            )));
        }
    }

    Ok(specs)
}

/// Parse the directive portion of a marker line (everything after the name),
/// e.g. `:batch :keyset $1=id $2=created_at`, into a cardinality and optional
/// keyset mappings.
fn parse_directives(
    rest: &str,
    name: &str,
    file: &Path,
) -> Result<(Cardinality, Vec<KeysetParam>, bool)> {
    let mut tokens = rest.split_whitespace().peekable();

    let card_tag = tokens
        .next()
        .and_then(|tok| tok.strip_prefix(':'))
        .ok_or_else(|| {
            ShkiError::config(format!(
                "Query '{}' in {} is missing a cardinality (:one, :many, :exec, or :batch)",
                name,
                file.display()
            ))
        })?;
    let cardinality = Cardinality::parse(card_tag).ok_or_else(|| {
        ShkiError::config(format!(
            "Unknown query cardinality ':{}' for query '{}' in {} (expected :one, :many, :exec, or :batch)",
            card_tag,
            name,
            file.display()
        ))
    })?;

    let mut keyset = Vec::new();
    let mut transaction = false;
    while let Some(token) = tokens.next() {
        match token {
            ":keyset" => {
                while let Some(next) = tokens.peek() {
                    if next.starts_with(':') {
                        break;
                    }
                    let reference = tokens.next().expect("peeked token");
                    let Some((parameter, field)) = reference.split_once('=') else {
                        return Err(ShkiError::config(format!(
                            "keyset mapping '{}' for query '{}' in {} must be $parameter=result_field, like $1=id",
                            reference,
                            name,
                            file.display()
                        )));
                    };
                    if !parameter.starts_with('$') || field.is_empty() {
                        return Err(ShkiError::config(format!(
                            "keyset mapping '{}' for query '{}' in {} must be $parameter=result_field, like $1=id",
                            reference,
                            name,
                            file.display()
                        )));
                    }
                    keyset.push(KeysetParam {
                        parameter: parameter.to_string(),
                        field: field.to_string(),
                    });
                }
                if keyset.is_empty() {
                    return Err(ShkiError::config(format!(
                        "':keyset' for query '{}' in {} requires at least one mapping (e.g. :keyset $1=id $2=created_at)",
                        name,
                        file.display()
                    )));
                }
            }
            ":tx" => transaction = true,
            other => {
                return Err(ShkiError::config(format!(
                    "Unknown directive '{}' for query '{}' in {}",
                    other,
                    name,
                    file.display()
                )));
            }
        }
    }

    if !keyset.is_empty() && cardinality != Cardinality::Batch {
        return Err(ShkiError::config(format!(
            "':keyset' for query '{}' in {} is only valid with :batch",
            name,
            file.display()
        )));
    }

    Ok((cardinality, keyset, transaction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_named_queries() {
        let content = r#"
-- file level comment, ignored

-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;

-- name: active_users :many
SELECT id, email
FROM users
WHERE active = true;

-- name: deactivate_user :exec
UPDATE users SET active = false WHERE id = $1;
"#;
        let specs = parse_query_file(content, Path::new("queries.sql")).expect("parse");
        assert_eq!(specs.len(), 3);

        assert_eq!(specs[0].name, "user_by_id");
        assert_eq!(specs[0].cardinality, Cardinality::One);
        assert!(!specs[0].transaction);
        assert_eq!(specs[0].sql, "SELECT * FROM users WHERE id = $1");

        assert_eq!(specs[1].name, "active_users");
        assert_eq!(specs[1].cardinality, Cardinality::Many);
        assert!(specs[1].sql.contains("WHERE active = true"));

        assert_eq!(specs[2].cardinality, Cardinality::Exec);
    }

    #[test]
    fn trailing_comment_lines_annotate_the_next_query() {
        let content = "-- name: first :one\n\
                       SELECT 1;\n\
                       \n\
                       -- doc comment for the second query\n\
                       -- name: second :one\n\
                       SELECT 2 -- inline comment stays\n";
        let specs = parse_query_file(content, Path::new("q.sql")).expect("parse");
        assert_eq!(specs[0].sql, "SELECT 1");
        assert_eq!(specs[1].sql, "SELECT 2 -- inline comment stays");
    }

    #[test]
    fn rejects_unknown_cardinality() {
        let content = "-- name: bad :several\nSELECT 1;";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("cardinality"));
    }

    #[test]
    fn rejects_empty_body() {
        let content = "-- name: empty :one\n";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("no SQL body"));
    }

    #[test]
    fn parses_keyset_modifier() {
        let content = "-- name: users_page :batch :keyset $1=id $2=created_at\n\
                       SELECT * FROM users WHERE (id, created_at) > ($1, $2) ORDER BY id LIMIT $3;";
        let specs = parse_query_file(content, Path::new("q.sql")).expect("parse");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].cardinality, Cardinality::Batch);
        assert_eq!(
            specs[0].keyset,
            vec![
                KeysetParam {
                    parameter: "$1".to_string(),
                    field: "id".to_string()
                },
                KeysetParam {
                    parameter: "$2".to_string(),
                    field: "created_at".to_string()
                },
            ]
        );
    }

    #[test]
    fn rejects_keyset_without_batch() {
        let content = "-- name: bad :many :keyset $1=id\nSELECT 1;";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("only valid with :batch"));
    }

    #[test]
    fn rejects_keyset_mapping_without_a_result_field() {
        let content = "-- name: bad :batch :keyset $1\nSELECT 1;";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("$parameter=result_field"));
    }

    #[test]
    fn rejects_keyset_mapping_without_a_parameter() {
        let content = "-- name: bad :batch :keyset id=id\nSELECT 1;";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("$parameter=result_field"));
    }

    #[test]
    fn parses_transaction_directive() {
        let content = "-- name: deactivate :exec :tx\nUPDATE users SET active = false;";
        let specs = parse_query_file(content, Path::new("q.sql")).expect("parse");
        assert!(specs[0].transaction);
    }

    #[test]
    fn rejects_unknown_directive() {
        let content = "-- name: bad :many :wat\nSELECT 1;";
        let err = parse_query_file(content, Path::new("q.sql")).expect_err("should reject");
        assert!(err.to_string().contains("Unknown directive"));
    }
}
