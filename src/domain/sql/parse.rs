/// Simplified parser to support declarative sql apply compile in shadow D
///
/// May expand to a more formal parser eventually
use std::path::PathBuf;

use squawk_lexer::TokenKind;

use crate::models::iden::Iden;
use crate::schema::{Constraint, ForeignKeyConstraint, Table};
use crate::sql::render::{SqlObjectType, SqlOperation, SqlStmt};
use crate::{Result, ShkiError};

pub fn parse_include_directive(line: &str) -> Result<Option<PathBuf>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('\\') {
        return Ok(None);
    }

    let Some(rest) = trimmed.strip_prefix("\\i") else {
        return Err(unsupported_backslash_command(trimmed));
    };

    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return Err(unsupported_backslash_command(trimmed));
    }

    let rest = strip_sql_line_comment(rest.trim());
    if rest.is_empty() {
        return Err(ShkiError::schema(
            "Declarative Schema include is missing a path",
        ));
    }

    Ok(Some(PathBuf::from(unquote_include_path(rest)?)))
}

fn strip_sql_line_comment(value: &str) -> &str {
    value
        .split_once("--")
        .map(|(value, _)| value)
        .unwrap_or(value)
        .trim()
}

fn unquote_include_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return Ok(value[1..value.len() - 1].to_string());
        }
    }

    if value.split_whitespace().count() > 1 {
        return Err(ShkiError::schema(format!(
            "Declarative Schema include paths with spaces must be quoted: {value}"
        )));
    }

    Ok(value.to_string())
}

fn unsupported_backslash_command(command: &str) -> ShkiError {
    let command = command.split_whitespace().next().unwrap_or(command);
    ShkiError::schema(format!(
        "Unsupported Declarative Schema command `{command}`. Only `\\i` includes are supported"
    ))
}

pub fn create_statement_object_type(statement: &str) -> SqlObjectType {
    create_statement_object_kind(statement)
        .and_then(|kind| match kind.as_str() {
            "SCHEMA" => Some(SqlObjectType::Schema),
            "EXTENSION" => Some(SqlObjectType::Extension),
            "TYPE" | "DOMAIN" => Some(SqlObjectType::Type),
            "FUNCTION" => Some(SqlObjectType::Function),
            "PROCEDURE" => Some(SqlObjectType::Procedure),
            "AGGREGATE" => Some(SqlObjectType::Aggregate),
            "SEQUENCE" => Some(SqlObjectType::Sequence),
            "VIEW" => Some(SqlObjectType::View),
            "MATERIALIZED VIEW" => Some(SqlObjectType::MaterializedView),
            "INDEX" => Some(SqlObjectType::Index),
            "TRIGGER" => Some(SqlObjectType::Trigger),
            "POLICY" => Some(SqlObjectType::Policy),
            _ => None,
        })
        .unwrap_or(SqlObjectType::Other)
}

pub fn create_statement_operation(statement: &str) -> SqlOperation {
    create_statement_object_kind(statement)
        .map(|_| SqlOperation::Create)
        .unwrap_or(SqlOperation::Raw)
}

pub fn create_table_info(statement: &str) -> Result<Option<Table>> {
    let Some(table_keyword_end) = create_table_keyword_end(statement) else {
        return Ok(None);
    };

    let Some(open_paren) = find_first_code_char(statement, TokenKind::OpenParen, table_keyword_end)
    else {
        return Ok(None);
    };
    let Some(close_paren) = find_matching_paren(statement, open_paren)? else {
        return Ok(None);
    };

    let table_name = create_table_name(statement, table_keyword_end, open_paren);
    let Some(table_id) = parse_relation_id(&table_name)? else {
        return Ok(None);
    };

    let mut table = match table_id.schema {
        Some(schema) => Table::in_schema(table_id.name, schema),
        None => Table::new(table_id.name),
    };

    let body = &statement[open_paren + 1..close_paren];
    for item in split_top_level_commas(body)? {
        if let Some(referenced) = referenced_table_id(&item)? {
            table.constraint(Constraint::ForeignKey(ForeignKeyConstraint::new(
                Vec::<String>::new(),
                referenced,
                Vec::<String>::new(),
            )));
        }
    }

    Ok(Some(table))
}

fn referenced_table_id(item: &str) -> Result<Option<Iden>> {
    let tokens = lex_sql(item)?;
    let Some(reference_token) = tokens.iter().find(|token| token.is_keyword("REFERENCES")) else {
        return Ok(None);
    };

    parse_relation_id(&item[reference_token.end..])
}

fn parse_relation_id(sql: &str) -> Result<Option<Iden>> {
    let tokens = lex_sql(sql)?;
    let mut parts = Vec::new();

    for token in tokens {
        if matches!(token.kind, TokenKind::OpenParen) {
            break;
        }
        if is_name_token(&token.kind) {
            parts.push(unquote_identifier(token.text));
            if parts.len() == 2 {
                break;
            }
        }
    }

    Ok(match parts.as_slice() {
        [name] => Some(Iden::new(name.clone(), None)),
        [schema, name] => Some(Iden::new(name.clone(), Some(schema.clone()))),
        _ => None,
    })
}

fn unquote_identifier(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\"\"", "\"")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenCreateTable {
    pub create_table_sql: String,
    pub deferred_foreign_keys: Vec<String>,
}

pub fn rewrite_create_table_foreign_keys(statement: &str) -> Result<Option<RewrittenCreateTable>> {
    let Some(table_keyword_end) = create_table_keyword_end(statement) else {
        return Ok(None);
    };

    let Some(open_paren) = find_first_code_char(statement, TokenKind::OpenParen, table_keyword_end)
    else {
        return Ok(None);
    };
    let Some(close_paren) = find_matching_paren(statement, open_paren)? else {
        return Ok(None);
    };

    let table_name = create_table_name(statement, table_keyword_end, open_paren);
    if table_name.is_empty() {
        return Ok(None);
    }

    let body = &statement[open_paren + 1..close_paren];
    let items = split_top_level_commas(body)?;
    let mut inline_items = Vec::new();
    let mut deferred_foreign_keys = Vec::new();

    for item in items {
        if is_table_level_foreign_key(&item) {
            deferred_foreign_keys.push(format!("ALTER TABLE {table_name} ADD {}", item.trim()));
        } else {
            inline_items.push(item.trim().to_string());
        }
    }

    if deferred_foreign_keys.is_empty() {
        return Ok(None);
    }

    let mut create_table_sql = String::new();
    create_table_sql.push_str(statement[..open_paren + 1].trim_end());
    create_table_sql.push('\n');
    create_table_sql.push_str(&inline_items.join(",\n"));
    create_table_sql.push('\n');
    create_table_sql.push_str(statement[close_paren..].trim_start());

    Ok(Some(RewrittenCreateTable {
        create_table_sql,
        deferred_foreign_keys,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenCreateIndex {
    /// The statement with `CONCURRENTLY` removed, safe to run inside the
    /// Shadow Database's single implicit transaction.
    pub sql: String,
    /// The declared index name, folded the way Postgres stores it (unquoted
    /// identifiers lowercased) so it matches introspected names.
    pub index_name: String,
}

/// Detect `CREATE [UNIQUE] INDEX CONCURRENTLY [IF NOT EXISTS] <name> ...`.
///
/// `CONCURRENTLY` refuses to run inside a transaction block, so a Declarative
/// Schema declaring it can't be applied to the Shadow Database verbatim — and
/// the flag isn't recorded in Postgres catalogs, so it wouldn't survive
/// introspection anyway. This strips the keyword for the apply and hands back
/// the index name so the compiler can re-mark the introspected index.
pub fn rewrite_create_index_concurrently(statement: &str) -> Option<RewrittenCreateIndex> {
    let tokens = lex_sql(statement).ok()?;
    let mut iter = SqlTokenIter::new(&tokens);

    let mut token = iter.next()?;
    if !token.is_keyword("CREATE") {
        return None;
    }
    token = iter.next()?;
    if token.is_keyword("UNIQUE") {
        token = iter.next()?;
    }
    if !token.is_keyword("INDEX") {
        return None;
    }
    let concurrently = iter.next()?;
    if !concurrently.is_keyword("CONCURRENTLY") {
        return None;
    }

    let mut name_token = iter.next()?;
    if name_token.is_keyword("IF") {
        if !iter.next()?.is_keyword("NOT") || !iter.next()?.is_keyword("EXISTS") {
            return None;
        }
        name_token = iter.next()?;
    }

    let index_name = if name_token.text.starts_with('"') {
        unquote_identifier(name_token.text)
    } else {
        name_token.text.to_lowercase()
    };

    Some(RewrittenCreateIndex {
        sql: format!(
            "{}{}",
            &statement[..concurrently.start],
            statement[concurrently.end..].trim_start()
        ),
        index_name,
    })
}

pub fn split_sql_statements(sql: &str) -> Result<Vec<String>> {
    let tokens = lex_sql(sql)?;
    let mut statements = Vec::new();
    let mut start = 0;
    for token in tokens {
        if matches!(token.kind, squawk_lexer::TokenKind::Semi) {
            let statement = sql[start..token.start].trim();
            if !statement.is_empty() {
                statements.push(statement.to_string());
            }
            start = token.end;
        }
    }

    let statement = sql[start..].trim();
    if !statement.is_empty() {
        statements.push(statement.to_string());
    }

    Ok(statements)
}

fn split_top_level_commas(sql: &str) -> Result<Vec<String>> {
    let tokens = lex_sql(sql)?;
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;

    for token in tokens {
        match token.kind {
            squawk_lexer::TokenKind::OpenParen => depth += 1,
            squawk_lexer::TokenKind::CloseParen => depth -= 1,
            squawk_lexer::TokenKind::Comma if depth == 0 => {
                let item = sql[start..token.start].trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                start = token.end;
            }
            _ => {}
        }
    }

    let item = sql[start..].trim();
    if !item.is_empty() {
        items.push(item.to_string());
    }

    Ok(items)
}

fn create_table_keyword_end(statement: &str) -> Option<usize> {
    let tokens = lex_sql(statement).ok()?;
    let mut tokens = SqlTokenIter::new(&tokens);
    let first = tokens.next()?;
    if !first.is_keyword("CREATE") {
        return None;
    }

    let mut next = tokens.next()?;
    while matches_any_keyword(&next, &["GLOBAL", "LOCAL", "TEMP", "TEMPORARY", "UNLOGGED"]) {
        next = tokens.next()?;
    }

    next.is_keyword("TABLE").then_some(next.end)
}

fn create_table_name(statement: &str, table_keyword_end: usize, open_paren: usize) -> String {
    let mut name = statement[table_keyword_end..open_paren].trim();
    if let Some(rest) = strip_leading_keywords(name, &["IF", "NOT", "EXISTS"]) {
        name = rest.trim();
    }
    name.to_string()
}

fn create_statement_object_kind(statement: &str) -> Option<String> {
    let Ok(tokens) = lex_sql(statement) else {
        return None;
    };
    let mut tokens = SqlTokenIter::new(&tokens);

    let first = tokens.next()?;
    if !first.is_keyword("CREATE") {
        return None;
    }

    let mut next = tokens.next()?;
    while matches_any_keyword(&next, &["OR", "REPLACE", "UNIQUE", "CONCURRENTLY"]) {
        next = tokens.next()?;
    }

    if next.is_keyword("MATERIALIZED") {
        return tokens
            .next()
            .filter(|token| token.is_keyword("VIEW"))
            .map(|_| "MATERIALIZED VIEW".to_string());
    }

    next.keyword_text().map(str::to_ascii_uppercase)
}

fn strip_leading_keywords<'a>(value: &'a str, keywords: &[&str]) -> Option<&'a str> {
    let tokens = lex_sql(value).ok()?;
    let mut tokens = SqlTokenIter::new(&tokens);
    let mut offset = 0;
    for expected in keywords {
        let token = tokens.next()?;
        if !token.is_keyword(expected) {
            return None;
        }
        offset = token.end;
    }
    Some(&value[offset..])
}

fn is_table_level_foreign_key(item: &str) -> bool {
    let Ok(tokens) = lex_sql(item) else {
        return false;
    };
    let mut tokens = SqlTokenIter::new(&tokens);
    let Some(first) = tokens.next() else {
        return false;
    };

    if first.is_keyword("FOREIGN") {
        return tokens.next().is_some_and(|token| token.is_keyword("KEY"));
    }

    if !first.is_keyword("CONSTRAINT") {
        return false;
    }

    tokens.next();
    tokens
        .next()
        .is_some_and(|token| token.is_keyword("FOREIGN"))
        && tokens.next().is_some_and(|token| token.is_keyword("KEY"))
}

pub fn is_alter_table_add_foreign_key(statement: &str) -> bool {
    let Ok(tokens) = lex_sql(statement) else {
        return false;
    };
    let tokens = SqlTokenIter::new(&tokens)
        .filter_map(|token| token.keyword_text().map(str::to_ascii_uppercase))
        .collect::<Vec<_>>();

    tokens.first().is_some_and(|token| token == "ALTER")
        && tokens.get(1).is_some_and(|token| token == "TABLE")
        && tokens.iter().any(|token| token == "ADD")
        && tokens.windows(2).any(|tokens| tokens == ["FOREIGN", "KEY"])
}

fn find_first_code_char(sql: &str, needle: TokenKind, start: usize) -> Option<usize> {
    // let kind = match needle {
    //     '(' => squawk_lexer::TokenKind::OpenParen,
    //     ')' => squawk_lexer::TokenKind::CloseParen,
    //     ',' => squawk_lexer::TokenKind::Comma,
    //     ';' => squawk_lexer::TokenKind::Semi,
    //     _ => return None,
    // };
    lex_sql(sql)
        .ok()?
        .into_iter()
        .find(|token| token.start >= start && token.kind == needle)
        .map(|token| token.start)
}

fn find_matching_paren(sql: &str, open_paren: usize) -> Result<Option<usize>> {
    let tokens = lex_sql(sql)?;
    let mut depth = 0_i32;
    for token in tokens.into_iter().filter(|token| token.start >= open_paren) {
        match token.kind {
            squawk_lexer::TokenKind::OpenParen => depth += 1,
            squawk_lexer::TokenKind::CloseParen => {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(token.start));
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

pub fn join_sql_statements(statements: &[SqlStmt]) -> String {
    statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
#[derive(Debug, Clone)]
struct SqlToken<'a> {
    kind: squawk_lexer::TokenKind,
    text: &'a str,
    start: usize,
    end: usize,
}

impl SqlToken<'_> {
    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(self.kind, squawk_lexer::TokenKind::Ident)
            && self.text.eq_ignore_ascii_case(keyword)
    }

    fn keyword_text(&self) -> Option<&str> {
        matches!(self.kind, squawk_lexer::TokenKind::Ident).then_some(self.text)
    }
}

struct SqlTokenIter<'a> {
    tokens: &'a [SqlToken<'a>],
    offset: usize,
}

impl<'a> SqlTokenIter<'a> {
    fn new(tokens: &'a [SqlToken<'a>]) -> Self {
        Self { tokens, offset: 0 }
    }
}

impl<'a> Iterator for SqlTokenIter<'a> {
    type Item = SqlToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.offset < self.tokens.len() {
            let token = self.tokens[self.offset].clone();
            self.offset += 1;
            if is_name_token(&token.kind) {
                return Some(token);
            }
        }
        None
    }
}

fn lex_sql(sql: &str) -> Result<Vec<SqlToken<'_>>> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    for token in squawk_lexer::tokenize(sql) {
        let len = token.len as usize;
        let end = offset + len;
        let text = &sql[offset..end];
        validate_sql_token(&token.kind)?;
        if !is_trivia_token(&token.kind) && !matches!(token.kind, squawk_lexer::TokenKind::Eof) {
            tokens.push(SqlToken {
                kind: token.kind,
                text,
                start: offset,
                end,
            });
        }
        offset = end;
    }
    Ok(tokens)
}

fn validate_sql_token(kind: &squawk_lexer::TokenKind) -> Result<()> {
    match kind {
        squawk_lexer::TokenKind::BlockComment { terminated: false } => Err(ShkiError::schema(
            "Unterminated block comment in Declarative Schema",
        )),
        squawk_lexer::TokenKind::QuotedIdent {
            terminated: false,
            uescape: _,
        } => Err(ShkiError::schema(
            "Unterminated quoted identifier in Declarative Schema",
        )),
        squawk_lexer::TokenKind::Literal { kind, .. } if literal_is_unterminated(kind) => Err(
            ShkiError::schema("Unterminated string literal in Declarative Schema"),
        ),
        _ => Ok(()),
    }
}

fn literal_is_unterminated(kind: &squawk_lexer::LiteralKind) -> bool {
    match kind {
        squawk_lexer::LiteralKind::Str { terminated }
        | squawk_lexer::LiteralKind::ByteStr { terminated }
        | squawk_lexer::LiteralKind::BitStr { terminated }
        | squawk_lexer::LiteralKind::DollarQuotedString { terminated }
        | squawk_lexer::LiteralKind::UnicodeEscStr { terminated }
        | squawk_lexer::LiteralKind::EscStr { terminated } => !terminated,
        _ => false,
    }
}

fn is_trivia_token(kind: &squawk_lexer::TokenKind) -> bool {
    matches!(
        kind,
        squawk_lexer::TokenKind::Whitespace
            | squawk_lexer::TokenKind::LineComment
            | squawk_lexer::TokenKind::BlockComment { .. }
    )
}

fn is_name_token(kind: &squawk_lexer::TokenKind) -> bool {
    matches!(
        kind,
        squawk_lexer::TokenKind::Ident | squawk_lexer::TokenKind::QuotedIdent { .. }
    )
}

fn matches_any_keyword(token: &SqlToken<'_>, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| token.is_keyword(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_create_index_concurrently_variants() {
        let cases = [
            (
                "CREATE INDEX CONCURRENTLY users_email_idx ON users (email)",
                "CREATE INDEX users_email_idx ON users (email)",
                "users_email_idx",
            ),
            (
                "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS \"MyIdx\" ON app.users (email)",
                "CREATE UNIQUE INDEX IF NOT EXISTS \"MyIdx\" ON app.users (email)",
                "MyIdx",
            ),
            (
                "create index concurrently Folded_Name on t (c)",
                "create index Folded_Name on t (c)",
                "folded_name",
            ),
        ];
        for (input, sql, name) in cases {
            let rewritten =
                rewrite_create_index_concurrently(input).expect("should detect CONCURRENTLY");
            assert_eq!(rewritten.sql, sql);
            assert_eq!(rewritten.index_name, name);
        }
    }

    #[test]
    fn plain_statements_are_not_rewritten() {
        for statement in [
            "CREATE INDEX users_email_idx ON users (email)",
            "CREATE TABLE concurrently (id int)",
            "DROP INDEX CONCURRENTLY users_email_idx",
        ] {
            assert!(rewrite_create_index_concurrently(statement).is_none());
        }
    }
}
