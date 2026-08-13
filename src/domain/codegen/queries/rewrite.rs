//! Rewrite named query parameters (`$id`) to Postgres positional placeholders
//! (`$1`).
//!
//! Postgres' wire protocol only understands positional placeholders, so before
//! describing or executing a query we replace each distinct `$name` with a `$n`
//! (first-appearance order; repeated names collapse to the same `$n`). The
//! `$name` form deliberately mirrors Postgres' own `$1` placeholders. The
//! scanner skips quoted strings, comments, and dollar-quoted bodies so it never
//! mistakes them for parameters, and distinguishes `$name` (a parameter) from
//! `$tag$...$tag$` dollar quoting (a `$ident` run followed by another `$`). See
//! `docs/adr/0001-typed-query-codegen.md`.

use crate::{Result, ShkiError};

/// Result of scanning a query for parameter placeholders.
#[derive(Debug)]
pub struct Rewritten {
    /// SQL with every `$name` replaced by its `$n` placeholder. Unchanged when
    /// the query already uses positional placeholders.
    pub sql: String,
    /// Parameter names in `$1..$n` order, or `None` when the query uses
    /// positional `$n` placeholders (no named parameters).
    pub names: Option<Vec<String>>,
}

/// Rewrite `$name` placeholders to `$n`, returning the rewritten SQL and the
/// ordered parameter names.
///
/// Errors if a query mixes named (`$name`) and positional (`$1`) placeholders,
/// since that makes the positional assignment ambiguous.
pub fn rewrite_named_params(sql: &str) -> Result<Rewritten> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut names: Vec<String> = Vec::new();
    let mut saw_positional = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // Single-quoted string literal: copy verbatim, honoring '' escapes.
            '\'' => i = copy_quoted(&chars, i, '\'', &mut out),
            // Double-quoted identifier: copy verbatim.
            '"' => i = copy_quoted(&chars, i, '"', &mut out),
            // Line comment `-- ... \n`.
            '-' if chars.get(i + 1) == Some(&'-') => i = copy_line_comment(&chars, i, &mut out),
            // Block comment `/* ... */`.
            '/' if chars.get(i + 1) == Some(&'*') => i = copy_block_comment(&chars, i, &mut out),
            // `$1` positional, `$name` parameter, or `$tag$...$tag$` dollar quote.
            '$' => i = handle_dollar(&chars, i, &mut out, &mut names, &mut saw_positional),
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    if !names.is_empty() && saw_positional {
        return Err(ShkiError::config(
            "query mixes named ($name) and positional ($1) parameters; use one style".to_string(),
        ));
    }

    Ok(Rewritten {
        sql: out,
        names: (!names.is_empty()).then_some(names),
    })
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Read an identifier starting at `start`, returning the name and the index just
/// past it.
fn read_ident(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut name = String::new();
    while i < chars.len() && is_ident_char(chars[i]) {
        name.push(chars[i]);
        i += 1;
    }
    (name, i)
}

/// Copy a `quote`-delimited run (string or identifier) verbatim, treating a
/// doubled quote as an escaped quote. Returns the index past the closing quote.
fn copy_quoted(chars: &[char], start: usize, quote: char, out: &mut String) -> usize {
    out.push(chars[start]);
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        if c == quote {
            if chars.get(i + 1) == Some(&quote) {
                // Escaped quote (`''` / `""`): copy the second and continue.
                out.push(quote);
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}

fn copy_line_comment(chars: &[char], start: usize, out: &mut String) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i] != '\n' {
        out.push(chars[i]);
        i += 1;
    }
    i
}

fn copy_block_comment(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push('/');
    out.push('*');
    let mut i = start + 2;
    while i < chars.len() {
        if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            out.push('*');
            out.push('/');
            return i + 2;
        }
        out.push(chars[i]);
        i += 1;
    }
    i
}

/// Handle a `$`: a positional placeholder (`$1`), a named parameter (`$name`), a
/// dollar-quoted string (`$$...$$` / `$tag$...$tag$`), or a bare `$`.
fn handle_dollar(
    chars: &[char],
    start: usize,
    out: &mut String,
    names: &mut Vec<String>,
    saw_positional: &mut bool,
) -> usize {
    // `$<digit>` is a positional placeholder.
    if chars.get(start + 1).is_some_and(|c| c.is_ascii_digit()) {
        *saw_positional = true;
        out.push('$');
        let mut i = start + 1;
        while i < chars.len() && chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
        }
        return i;
    }

    // The identifier run after `$` (may be empty, e.g. for `$$`).
    let (run, after) = read_ident(chars, start + 1);

    // A `$ident$` (or `$$`) sequence opens a dollar-quoted string: copy the whole
    // body verbatim through the matching close delimiter.
    if chars.get(after) == Some(&'$') {
        let tag: Vec<char> = chars[start..=after].to_vec();
        out.extend(&tag);
        let mut k = after + 1;
        while k < chars.len() {
            if chars[k..].starts_with(&tag[..]) {
                out.extend(&tag);
                return k + tag.len();
            }
            out.push(chars[k]);
            k += 1;
        }
        return k;
    }

    // `$name` (not followed by `$`) is a named parameter.
    if !run.is_empty() {
        let idx = match names.iter().position(|existing| existing == &run) {
            Some(pos) => pos + 1,
            None => {
                names.push(run);
                names.len()
            }
        };
        out.push('$');
        out.push_str(&idx.to_string());
        return after;
    }

    // Bare `$`.
    out.push('$');
    start + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(sql: &str) -> Rewritten {
        rewrite_named_params(sql).expect("rewrite")
    }

    #[test]
    fn positional_query_is_unchanged() {
        let r = rewrite("SELECT * FROM users WHERE id = $1 AND age > $2");
        assert_eq!(r.sql, "SELECT * FROM users WHERE id = $1 AND age > $2");
        assert!(r.names.is_none());
    }

    #[test]
    fn named_params_become_positional() {
        let r = rewrite("SELECT * FROM items WHERE id = $id");
        assert_eq!(r.sql, "SELECT * FROM items WHERE id = $1");
        assert_eq!(r.names, Some(vec!["id".to_string()]));
    }

    #[test]
    fn repeated_name_reuses_index() {
        let r = rewrite("SELECT * FROM t WHERE a = $x OR b = $y OR c = $x");
        assert_eq!(r.sql, "SELECT * FROM t WHERE a = $1 OR b = $2 OR c = $1");
        assert_eq!(r.names, Some(vec!["x".to_string(), "y".to_string()]));
    }

    #[test]
    fn leaves_type_casts_untouched() {
        let r = rewrite("SELECT id::text, $val FROM t");
        assert_eq!(r.sql, "SELECT id::text, $1 FROM t");
        assert_eq!(r.names, Some(vec!["val".to_string()]));
    }

    #[test]
    fn skips_strings_and_comments() {
        let r = rewrite("SELECT '$notparam' -- $nope\nFROM t WHERE x = $real");
        assert_eq!(r.sql, "SELECT '$notparam' -- $nope\nFROM t WHERE x = $1");
        assert_eq!(r.names, Some(vec!["real".to_string()]));
    }

    #[test]
    fn skips_block_comments_and_quoted_identifiers() {
        let r = rewrite("SELECT \"$column\" /* $ignored */ FROM t WHERE x = $value");
        assert_eq!(
            r.sql,
            "SELECT \"$column\" /* $ignored */ FROM t WHERE x = $1"
        );
        assert_eq!(r.names, Some(vec!["value".to_string()]));
    }

    #[test]
    fn skips_dollar_quoted_body() {
        let r = rewrite("SELECT $$ $notparam $$, $real FROM t");
        assert_eq!(r.sql, "SELECT $$ $notparam $$, $1 FROM t");
        assert_eq!(r.names, Some(vec!["real".to_string()]));
    }

    #[test]
    fn tagged_dollar_quote_is_not_a_named_param() {
        let r = rewrite("SELECT $tag$ body $tag$, $p FROM t");
        assert_eq!(r.sql, "SELECT $tag$ body $tag$, $1 FROM t");
        assert_eq!(r.names, Some(vec!["p".to_string()]));
    }

    #[test]
    fn rejects_mixed_styles() {
        let err = rewrite_named_params("SELECT * FROM t WHERE a = $1 AND b = $b")
            .expect_err("should reject mixing");
        assert!(err.to_string().contains("mixes named"));
    }
}
