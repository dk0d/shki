use std::collections::HashSet;
use std::path::{Path, PathBuf};

use squawk_lexer::TokenKind;

use crate::{Result, ShkiError};

pub const DIRECTORY_SCHEMA_ENTRYPOINT: &str = "main.sql";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeSchema {
    pub entrypoint: PathBuf,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeApplySql {
    pub setup_sql: String,
    pub deferred_sql: String,
}

pub fn normalize_declarative_apply_sql(sql: &str) -> Result<String> {
    let plan = plan_declarative_apply_sql(sql)?;
    Ok([plan.setup_sql, plan.deferred_sql]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn plan_declarative_apply_sql(sql: &str) -> Result<DeclarativeApplySql> {
    let mut setup = Vec::new();
    let mut deferred = Vec::new();

    for statement in split_sql_statements(sql)? {
        if let Some(rewritten) = rewrite_create_table_foreign_keys(&statement)? {
            setup.push(rewritten.create_table_sql);
            deferred.extend(rewritten.deferred_foreign_keys);
        } else if is_alter_table_add_foreign_key(&statement) {
            deferred.push(statement);
        } else {
            setup.push(statement);
        }
    }

    Ok(DeclarativeApplySql {
        setup_sql: join_sql_statements(&setup),
        deferred_sql: join_sql_statements(&deferred),
    })
}

pub fn load_declarative_schema(path: impl AsRef<Path>) -> Result<DeclarativeSchema> {
    let path = path.as_ref();
    let entrypoint = if path.is_dir() {
        path.join(DIRECTORY_SCHEMA_ENTRYPOINT)
    } else {
        path.to_path_buf()
    };

    if !entrypoint.exists() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint does not exist: {}",
            entrypoint.display()
        )));
    }

    if !entrypoint.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint is not a file: {}",
            entrypoint.display()
        )));
    }

    let mut loading = Vec::new();
    let mut loaded = HashSet::new();
    let sql = load_sql_file(&entrypoint, &mut loading, &mut loaded)?;

    Ok(DeclarativeSchema { entrypoint, sql })
}

fn load_sql_file(
    path: &Path,
    loading: &mut Vec<PathBuf>,
    loaded: &mut HashSet<PathBuf>,
) -> Result<String> {
    let canonical = canonicalize_existing_file(path)?;

    if let Some(index) = loading.iter().position(|active| active == &canonical) {
        let mut cycle = loading[index..]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        cycle.push(canonical.display().to_string());
        return Err(ShkiError::schema(format!(
            "Cyclic Declarative Schema include detected: {}",
            cycle.join(" -> ")
        )));
    }

    if loaded.contains(&canonical) {
        return Ok(String::new());
    }

    loading.push(canonical.clone());
    let content = std::fs::read_to_string(&canonical)?;
    let mut expanded = String::new();

    for line in content.lines() {
        if let Some(include_path) = parse_include_directive(line)? {
            let include_path = canonical
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(include_path);
            expanded.push_str(&load_sql_file(&include_path, loading, loaded)?);
            if !expanded.ends_with('\n') {
                expanded.push('\n');
            }
        } else {
            expanded.push_str(line);
            expanded.push('\n');
        }
    }

    loading.pop();
    loaded.insert(canonical);
    Ok(expanded)
}

fn canonicalize_existing_file(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|err| {
        ShkiError::schema(format!(
            "Failed to read Declarative Schema file {}: {}",
            path.display(),
            err
        ))
    })?;

    if !canonical.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema include is not a file: {}",
            canonical.display()
        )));
    }

    Ok(canonical)
}

fn parse_include_directive(line: &str) -> Result<Option<PathBuf>> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RewrittenCreateTable {
    create_table_sql: String,
    deferred_foreign_keys: Vec<String>,
}

fn rewrite_create_table_foreign_keys(statement: &str) -> Result<Option<RewrittenCreateTable>> {
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

fn split_sql_statements(sql: &str) -> Result<Vec<String>> {
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

fn is_alter_table_add_foreign_key(statement: &str) -> bool {
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

fn join_sql_statements(statements: &[String]) -> String {
    statements
        .iter()
        .map(|statement| format!("{};", statement.trim().trim_end_matches(';')))
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
        squawk_lexer::TokenKind::QuotedIdent { terminated: false } => Err(ShkiError::schema(
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
    use tempfile::TempDir;

    #[test]
    fn loads_single_sql_file() {
        let temp = TempDir::new().expect("temp dir");
        let schema = temp.path().join("schema.sql");
        std::fs::write(&schema, "CREATE TABLE users (id int);\n").expect("write schema");

        let loaded = load_declarative_schema(&schema).expect("load schema");

        assert_eq!(loaded.entrypoint, schema);
        assert_eq!(loaded.sql, "CREATE TABLE users (id int);\n");
    }

    #[test]
    fn loads_directory_schema_from_main_sql() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "SELECT 1;\n").expect("write main");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.entrypoint, temp.path().join("main.sql"));
        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn expands_relative_i_includes() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::create_dir(temp.path().join("tables")).expect("create tables dir");
        std::fs::write(
            temp.path().join("main.sql"),
            "CREATE SCHEMA app;\n\\i tables/users.sql\n",
        )
        .expect("write main");
        std::fs::write(
            temp.path().join("tables/users.sql"),
            "CREATE TABLE app.users (id int);\n",
        )
        .expect("write users");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(
            loaded.sql,
            "CREATE SCHEMA app;\nCREATE TABLE app.users (id int);\n"
        );
    }

    #[test]
    fn supports_quoted_include_paths() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i 'user table.sql'\n").expect("write main");
        std::fs::write(temp.path().join("user table.sql"), "SELECT 1;\n").expect("write file");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn rejects_include_cycles() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i a.sql\n").expect("write main");
        std::fs::write(temp.path().join("a.sql"), "\\i b.sql\n").expect("write a");
        std::fs::write(temp.path().join("b.sql"), "\\i a.sql\n").expect("write b");

        let error = load_declarative_schema(temp.path()).expect_err("cycle should fail");

        assert!(
            error
                .to_string()
                .contains("Cyclic Declarative Schema include")
        );
    }

    #[test]
    fn rejects_unsupported_backslash_commands() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("schema.sql"), "\\ir other.sql\n").expect("write schema");

        let error = load_declarative_schema(temp.path().join("schema.sql"))
            .expect_err("unsupported command should fail");

        assert!(
            error
                .to_string()
                .contains("Only `\\i` includes are supported")
        );
    }

    #[test]
    fn normalizes_inline_table_foreign_keys_to_deferred_alter_table() {
        let sql = r#"
CREATE TABLE "public"."enrollment_session_to_fingerprint" (
  "enrollment_session_id" UUID NOT NULL,
  "fingerprint_id" UUID NOT NULL,
  CONSTRAINT "fk_session" FOREIGN KEY ("enrollment_session_id") REFERENCES "public"."enrollment_session" ("id"),
  CONSTRAINT "fk_fingerprint" FOREIGN KEY ("fingerprint_id") REFERENCES "public"."fingerprint" ("id"),
  CONSTRAINT "pk_join" PRIMARY KEY ("enrollment_session_id", "fingerprint_id")
);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert!(
            plan.setup_sql
                .contains("CONSTRAINT \"pk_join\" PRIMARY KEY")
        );
        assert!(
            !plan
                .setup_sql
                .contains("CONSTRAINT \"fk_session\" FOREIGN KEY")
        );
        assert!(plan.deferred_sql.contains(
            "ALTER TABLE \"public\".\"enrollment_session_to_fingerprint\" ADD CONSTRAINT \"fk_session\" FOREIGN KEY"
        ));
        assert!(plan.deferred_sql.contains(
            "ALTER TABLE \"public\".\"enrollment_session_to_fingerprint\" ADD CONSTRAINT \"fk_fingerprint\" FOREIGN KEY"
        ));
    }

    #[test]
    fn normalizes_standalone_alter_table_foreign_keys_to_deferred_sql() {
        let sql = r#"
ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (parent_id) REFERENCES parent(id);
CREATE TABLE child (parent_id int);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, "CREATE TABLE child (parent_id int);");
        assert_eq!(
            plan.deferred_sql,
            "ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (parent_id) REFERENCES parent(id);"
        );
    }

    #[test]
    fn normalizes_quoted_foreign_key_constraint_names() {
        let sql = r#"
CREATE TABLE child (
  parent_id int,
  CONSTRAINT "child_parent_fkey" FOREIGN KEY (parent_id) REFERENCES parent(id)
);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, "CREATE TABLE child (\nparent_id int\n);");
        assert_eq!(
            plan.deferred_sql,
            "ALTER TABLE child ADD CONSTRAINT \"child_parent_fkey\" FOREIGN KEY (parent_id) REFERENCES parent(id);"
        );
    }

    #[test]
    fn alter_table_literals_do_not_trigger_foreign_key_deferral() {
        let sql = "ALTER TABLE child ADD COLUMN note text DEFAULT 'FOREIGN KEY';";

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, sql);
        assert!(plan.deferred_sql.is_empty());
    }

    #[test]
    fn normalizer_preserves_commas_and_semicolons_inside_sql_literals() {
        let sql = r#"
CREATE TABLE child (
  id int,
  note text DEFAULT 'a,b;c',
  CONSTRAINT child_parent_fkey FOREIGN KEY (id) REFERENCES parent(id)
);
CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  RAISE NOTICE 'not a statement; still function body';
END;
$$;
"#;

        let normalized = normalize_declarative_apply_sql(sql).expect("sql should normalize");

        assert!(normalized.contains("note text DEFAULT 'a,b;c'"));
        assert!(normalized.contains("RAISE NOTICE 'not a statement; still function body';"));
        assert!(normalized.contains(
            "ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (id) REFERENCES parent(id);"
        ));
    }
}
