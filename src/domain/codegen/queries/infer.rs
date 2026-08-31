//! Infer parameter nullability from the schema column a parameter feeds.
//!
//! When a parameter is the entire value written to a column — `INSERT ... (a,
//! b) VALUES ($1, $2)` or `UPDATE ... SET a = $1` — and the Declarative Schema
//! says that column is nullable, the generated argument becomes `Option<T>`.
//! Anything else (expressions, casts, comparisons) is left alone: a nullable
//! column in a `WHERE` comparison does not make a `NULL` bind useful, since
//! `col = NULL` never matches.
//!
//! This is a token-level scan, not a SQL parser: it only recognizes the two
//! shapes above and maps nothing when a statement doesn't match.

use std::collections::HashSet;

use squawk_lexer::TokenKind;

#[derive(Clone)]
struct Tok<'a> {
    kind: TokenKind,
    text: &'a str,
}

fn lex(sql: &str) -> Vec<Tok<'_>> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    for token in squawk_lexer::tokenize(sql) {
        let end = offset + token.len as usize;
        let text = &sql[offset..end];
        match token.kind {
            TokenKind::Whitespace
            | TokenKind::LineComment
            | TokenKind::BlockComment { .. }
            | TokenKind::Eof => {}
            kind => tokens.push(Tok { kind, text }),
        }
        offset = end;
    }
    tokens
}

fn is_kw(token: &Tok, keyword: &str) -> bool {
    matches!(token.kind, TokenKind::Ident) && token.text.eq_ignore_ascii_case(keyword)
}

/// The name an identifier token denotes: unquoted identifiers fold to
/// lowercase (as Postgres does), quoted identifiers are taken verbatim.
fn ident_text(token: &Tok) -> Option<String> {
    match token.kind {
        TokenKind::Ident => Some(token.text.to_lowercase()),
        TokenKind::QuotedIdent { .. } => Some(token.text.trim_matches('"').to_string()),
        _ => None,
    }
}

/// Zero-based parameter index of a `$n` token.
fn param_index(token: &Tok) -> Option<usize> {
    matches!(token.kind, TokenKind::PositionalParam { .. })
        .then(|| token.text[1..].parse::<usize>().ok())
        .flatten()
        .and_then(|n| n.checked_sub(1))
}

/// Does this token end a `SET` assignment list?
fn ends_set_list(token: &Tok) -> bool {
    ["where", "from", "returning"]
        .iter()
        .any(|kw| is_kw(token, kw))
        || matches!(token.kind, TokenKind::Semi)
}

/// Scan `sql` (positional-placeholder form) for parameters written whole into
/// a column, and return the zero-based indices of those whose column
/// `is_nullable(table, column)` reports nullable.
pub fn nullable_target_params(
    sql: &str,
    is_nullable: impl Fn(&str, &str) -> Option<bool>,
) -> HashSet<usize> {
    let tokens = lex(sql);
    let mut nullable = HashSet::new();
    let mut mark = |table: &str, column: &str, param: usize| {
        if is_nullable(table, column) == Some(true) {
            nullable.insert(param);
        }
    };

    // The most recent INSERT target, for `ON CONFLICT ... DO UPDATE SET`.
    let mut insert_table: Option<String> = None;
    let mut i = 0;
    while i < tokens.len() {
        if is_kw(&tokens[i], "insert") && tokens.get(i + 1).is_some_and(|t| is_kw(t, "into")) {
            i = scan_insert(&tokens, i + 2, &mut insert_table, &mut mark);
        } else if is_kw(&tokens[i], "update") {
            i = scan_update(&tokens, i + 1, insert_table.as_deref(), &mut mark);
        } else {
            i += 1;
        }
    }
    nullable
}

/// Read a possibly schema-qualified name starting at `i`, returning its last
/// segment and the index past it.
fn read_qualified_name(tokens: &[Tok], mut i: usize) -> (Option<String>, usize) {
    let mut name = None;
    while let Some(text) = tokens.get(i).and_then(ident_text) {
        name = Some(text);
        i += 1;
        if tokens.get(i).is_some_and(|t| matches!(t.kind, TokenKind::Dot)) {
            i += 1;
        } else {
            break;
        }
    }
    (name, i)
}

/// Scan `INSERT INTO <table> (<columns>) VALUES (<exprs>)[, (<exprs>)]...`,
/// mapping each expression that is exactly one parameter to its column.
fn scan_insert(
    tokens: &[Tok],
    start: usize,
    insert_table: &mut Option<String>,
    mark: &mut impl FnMut(&str, &str, usize),
) -> usize {
    let (Some(table), mut i) = read_qualified_name(tokens, start) else {
        return start + 1;
    };
    *insert_table = Some(table.clone());

    // Explicit column list; without one, the columns are the table's in order,
    // which we cannot know here — only the schema lookup caller could, and an
    // INSERT without a column list is rare enough to skip.
    let mut columns: Vec<String> = Vec::new();
    if tokens
        .get(i)
        .is_some_and(|t| matches!(t.kind, TokenKind::OpenParen))
    {
        i += 1;
        while let Some(token) = tokens.get(i) {
            if matches!(token.kind, TokenKind::CloseParen) {
                i += 1;
                break;
            }
            if let Some(column) = ident_text(token) {
                columns.push(column);
            }
            i += 1;
        }
    }

    if columns.is_empty() || !tokens.get(i).is_some_and(|t| is_kw(t, "values")) {
        return i;
    }
    i += 1;

    // One or more parenthesized tuples, comma separated.
    while tokens
        .get(i)
        .is_some_and(|t| matches!(t.kind, TokenKind::OpenParen))
    {
        i += 1;
        let mut position = 0;
        let mut expr_tokens = 0;
        let mut only_param: Option<usize> = None;
        let mut depth = 0;
        while let Some(token) = tokens.get(i) {
            match token.kind {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen if depth > 0 => depth -= 1,
                TokenKind::CloseParen | TokenKind::Comma if depth == 0 => {
                    if expr_tokens == 1
                        && let Some(param) = only_param
                        && let Some(column) = columns.get(position)
                    {
                        mark(insert_table.as_deref().unwrap_or_default(), column, param);
                    }
                    position += 1;
                    expr_tokens = 0;
                    only_param = None;
                    if matches!(token.kind, TokenKind::CloseParen) {
                        i += 1;
                        break;
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
            expr_tokens += 1;
            only_param = param_index(token).filter(|_| expr_tokens == 1);
            i += 1;
        }
        if tokens
            .get(i)
            .is_some_and(|t| matches!(t.kind, TokenKind::Comma))
        {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// Scan `UPDATE <table> ... SET col = $n, ...`, mapping assignments whose
/// entire value is one parameter. `UPDATE` directly followed by `SET` is an
/// `ON CONFLICT ... DO UPDATE`, which targets the enclosing INSERT's table.
fn scan_update(
    tokens: &[Tok],
    start: usize,
    insert_table: Option<&str>,
    mark: &mut impl FnMut(&str, &str, usize),
) -> usize {
    let mut i = start;
    let table = if tokens.get(i).is_some_and(|t| is_kw(t, "set")) {
        let Some(table) = insert_table else {
            return start;
        };
        table.to_string()
    } else {
        let (Some(table), next) = read_qualified_name(tokens, i) else {
            return start;
        };
        i = next;
        table
    };

    // Skip an optional alias to reach SET.
    while let Some(token) = tokens.get(i) {
        if is_kw(token, "set") {
            i += 1;
            break;
        }
        if !is_kw(token, "as") && ident_text(token).is_none() {
            return i;
        }
        i += 1;
    }

    // Assignments: `col = <expr>` separated by top-level commas, ending at
    // WHERE/FROM/RETURNING or end of statement.
    loop {
        let Some(column) = tokens.get(i).and_then(ident_text) else {
            return i;
        };
        if !tokens
            .get(i + 1)
            .is_some_and(|t| matches!(t.kind, TokenKind::Eq))
        {
            return i;
        }
        i += 2;
        let value_is_param = tokens.get(i).and_then(param_index);
        let terminated = tokens
            .get(i + 1)
            .is_none_or(|t| matches!(t.kind, TokenKind::Comma) || ends_set_list(t));
        if let Some(param) = value_is_param
            && terminated
        {
            mark(&table, &column, param);
        }
        // Skip to the next top-level comma or the end of the SET list.
        let mut depth = 0;
        while let Some(token) = tokens.get(i) {
            match token.kind {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => depth -= 1,
                TokenKind::Comma if depth == 0 => {
                    i += 1;
                    break;
                }
                _ if depth == 0 && ends_set_list(token) => return i,
                _ => {}
            }
            i += 1;
        }
        if i >= tokens.len() {
            return i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// users(id NOT NULL, email NOT NULL, bio nullable, annotation nullable)
    fn lookup(table: &str, column: &str) -> Option<bool> {
        (table == "users").then(|| matches!(column, "bio" | "annotation"))
    }

    fn infer(sql: &str) -> HashSet<usize> {
        nullable_target_params(sql, lookup)
    }

    #[test]
    fn insert_params_map_to_their_columns() {
        let nullable = infer("INSERT INTO users (id, bio, email) VALUES ($1, $2, $3)");
        assert_eq!(nullable, HashSet::from([1]));
    }

    #[test]
    fn update_set_params_map_to_their_columns() {
        let nullable = infer("UPDATE users SET bio = $1, email = $2 WHERE id = $3");
        assert_eq!(nullable, HashSet::from([0]));
    }

    #[test]
    fn on_conflict_do_update_targets_the_insert_table() {
        let nullable = infer(
            "INSERT INTO users (id, email) VALUES ($1, $2) \
             ON CONFLICT (id) DO UPDATE SET annotation = $3",
        );
        assert_eq!(nullable, HashSet::from([2]));
    }

    #[test]
    fn expressions_and_casts_are_not_mapped() {
        // A parameter inside an expression is not the whole column value.
        let nullable = infer("INSERT INTO users (id, bio) VALUES ($1, coalesce($2, 'x'))");
        assert!(nullable.is_empty());
        let nullable = infer("UPDATE users SET bio = $1::text || 'x'");
        assert!(nullable.is_empty());
    }

    #[test]
    fn where_comparisons_are_not_mapped() {
        let nullable = infer("SELECT * FROM users WHERE bio = $1");
        assert!(nullable.is_empty());
    }

    #[test]
    fn multi_row_values_map_every_tuple() {
        let nullable = infer("INSERT INTO users (id, bio) VALUES ($1, $2), ($3, $4)");
        assert_eq!(nullable, HashSet::from([1, 3]));
    }

    #[test]
    fn unknown_tables_map_nothing() {
        let nullable = infer("INSERT INTO missing (bio) VALUES ($1)");
        assert!(nullable.is_empty());
    }

    #[test]
    fn qualified_names_and_keyword_case_are_normalized() {
        let nullable = infer("INSERT INTO Public.USERS (BIO) VALUES ($1)");
        assert_eq!(nullable, HashSet::from([0]));
        let nullable = infer("update public.users set bio = $1");
        assert_eq!(nullable, HashSet::from([0]));
    }

    #[test]
    fn quoted_identifiers_are_matched_verbatim() {
        let nullable = infer("INSERT INTO \"users\" (\"bio\") VALUES ($1)");
        assert_eq!(nullable, HashSet::from([0]));
    }

    #[test]
    fn update_with_alias_from_and_returning() {
        let nullable = infer(
            "UPDATE users AS u SET bio = $1 FROM users v WHERE u.id = v.id RETURNING u.id",
        );
        assert_eq!(nullable, HashSet::from([0]));
        let nullable = infer("UPDATE users u SET bio = $1 RETURNING id");
        assert_eq!(nullable, HashSet::from([0]));
    }

    #[test]
    fn insert_with_returning_still_maps() {
        let nullable = infer("INSERT INTO users (id, bio) VALUES ($1, $2) RETURNING id");
        assert_eq!(nullable, HashSet::from([1]));
    }
}
