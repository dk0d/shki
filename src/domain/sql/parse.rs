//! Statement-level pre-pass over the Declarative Schema, built on
//! squawk-syntax's lossless, error-tolerant Postgres parse tree.
//!
//! The Shadow Database stays the authority on SQL validity: `SourceFile::parse`
//! always produces a tree (unparseable stretches become error nodes that keep
//! their text), so statements the grammar doesn't know pass through verbatim
//! instead of being rejected here.

use std::path::PathBuf;

use squawk_syntax::ast::{self, AstNode};
use squawk_syntax::{SourceFile, SyntaxKind, SyntaxNode, SyntaxToken};

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

/// Split SQL into statements. Statement boundaries come from the parse tree,
/// so semicolons inside literals, dollar-quoted bodies, and `BEGIN ATOMIC`
/// blocks don't split; unparseable stretches survive as their raw text.
pub fn split_sql_statements(sql: &str) -> Result<Vec<String>> {
    let parse = SourceFile::parse(sql);
    Ok(parse
        .syntax_node()
        .children()
        .filter_map(|node| {
            let text = node.text().to_string();
            let text = text.trim().trim_end_matches(';').trim_end();
            (!text.is_empty()).then(|| text.to_string())
        })
        .collect())
}

/// The first statement of `statement`, if the grammar recognizes it.
fn first_stmt(statement: &str) -> Option<ast::Stmt> {
    SourceFile::parse(statement).tree().stmts().next()
}

/// The first statement of `statement` as a specific node type.
fn typed_stmt<N: AstNode>(statement: &str) -> Option<N> {
    SourceFile::parse(statement)
        .syntax_node()
        .children()
        .find_map(N::cast)
}

pub fn create_statement_object_type(statement: &str) -> SqlObjectType {
    match first_stmt(statement) {
        Some(ast::Stmt::CreateSchema(_)) => SqlObjectType::Schema,
        Some(ast::Stmt::CreateExtension(_)) => SqlObjectType::Extension,
        Some(ast::Stmt::CreateType(_) | ast::Stmt::CreateDomain(_)) => SqlObjectType::Type,
        Some(ast::Stmt::CreateFunction(_)) => SqlObjectType::Function,
        Some(ast::Stmt::CreateProcedure(_)) => SqlObjectType::Procedure,
        Some(ast::Stmt::CreateAggregate(_)) => SqlObjectType::Aggregate,
        Some(ast::Stmt::CreateSequence(_)) => SqlObjectType::Sequence,
        Some(ast::Stmt::CreateView(_)) => SqlObjectType::View,
        Some(ast::Stmt::CreateMaterializedView(_)) => SqlObjectType::MaterializedView,
        Some(ast::Stmt::CreateIndex(_)) => SqlObjectType::Index,
        Some(ast::Stmt::CreateTrigger(_)) => SqlObjectType::Trigger,
        Some(ast::Stmt::CreatePolicy(_)) => SqlObjectType::Policy,
        _ => SqlObjectType::Other,
    }
}

pub fn create_statement_operation(statement: &str) -> SqlOperation {
    let is_create = first_stmt(statement).is_some_and(|stmt| {
        first_code_token(stmt.syntax()).is_some_and(|token| token.kind() == SyntaxKind::CREATE_KW)
    });
    if is_create {
        SqlOperation::Create
    } else {
        SqlOperation::Raw
    }
}

fn first_code_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
}

/// Identifier tokens under `node`, unquoted, in source order.
fn name_parts(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| unquote_identifier(token.text()))
        .collect()
}

/// A `[schema.]name` identifier from a name/path node.
fn node_iden(node: &SyntaxNode) -> Option<Iden> {
    let mut parts = name_parts(node);
    let name = parts.pop()?;
    Some(Iden::new(name, parts.pop()))
}

pub fn create_table_info(statement: &str) -> Result<Option<Table>> {
    let Some(create) = typed_stmt::<ast::CreateTable>(statement) else {
        return Ok(None);
    };
    let Some(table_id) = create
        .table_name()
        .and_then(|name| node_iden(name.syntax()))
    else {
        return Ok(None);
    };

    let mut table = match table_id.schema {
        Some(schema) => Table::in_schema(table_id.name, schema),
        None => Table::new(table_id.name),
    };

    for node in create.syntax().descendants() {
        let referenced = if let Some(fk) = ast::ForeignKeyConstraint::cast(node.clone()) {
            fk.table_name_ref().and_then(|r| node_iden(r.syntax()))
        } else if ast::ReferencesConstraint::can_cast(node.kind()) {
            node.children()
                .find_map(ast::TableNameRef::cast)
                .and_then(|r| node_iden(r.syntax()))
        } else {
            None
        };
        if let Some(referenced) = referenced {
            table.constraint(Constraint::ForeignKey(ForeignKeyConstraint::new(
                Vec::<String>::new(),
                referenced,
                Vec::<String>::new(),
            )));
        }
    }

    Ok(Some(table))
}

fn unquote_identifier(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\"\"", "\"")
    } else {
        value.to_string()
    }
}

/// Fold an identifier the way Postgres stores it: quoted kept verbatim,
/// unquoted lowercased.
fn fold_identifier(text: &str) -> String {
    if text.starts_with('"') {
        unquote_identifier(text)
    } else {
        text.to_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenCreateTable {
    pub create_table_sql: String,
    pub deferred_foreign_keys: Vec<String>,
}

pub fn rewrite_create_table_foreign_keys(statement: &str) -> Result<Option<RewrittenCreateTable>> {
    let Some(create) = typed_stmt::<ast::CreateTable>(statement) else {
        return Ok(None);
    };
    let (Some(table_name), Some(arg_list)) = (create.table_name(), create.table_arg_list()) else {
        return Ok(None);
    };
    let (Some(open_paren), Some(close_paren)) =
        (arg_list.l_paren_token(), arg_list.r_paren_token())
    else {
        return Ok(None);
    };

    let table_name = table_name.syntax().text().to_string();
    let mut inline_items = Vec::new();
    let mut deferred_foreign_keys = Vec::new();

    for arg in arg_list.args() {
        match &arg {
            ast::TableArg::TableConstraint(ast::TableConstraint::ForeignKeyConstraint(fk)) => {
                deferred_foreign_keys.push(format!(
                    "ALTER TABLE {table_name} ADD {}",
                    fk.syntax().text().to_string().trim()
                ));
            }
            other => inline_items.push(other.syntax().text().to_string().trim().to_string()),
        }
    }

    if deferred_foreign_keys.is_empty() {
        return Ok(None);
    }

    let open_paren = usize::from(open_paren.text_range().start());
    let close_paren = usize::from(close_paren.text_range().start());
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

pub fn is_alter_table_add_foreign_key(statement: &str) -> bool {
    typed_stmt::<ast::AlterTable>(statement).is_some_and(|alter| {
        alter.syntax().descendants().any(|node| {
            ast::AddConstraint::cast(node)
                .and_then(|add| add.constraint())
                .is_some_and(|constraint| {
                    matches!(constraint, ast::Constraint::ForeignKeyConstraint(_))
                })
        })
    })
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

/// Detect `CREATE [UNIQUE] INDEX CONCURRENTLY [IF NOT EXISTS] [<name>] ...`.
///
/// `CONCURRENTLY` refuses to run inside a transaction block, so a Declarative
/// Schema declaring it can't be applied to the Shadow Database verbatim — and
/// the flag isn't recorded in Postgres catalogs, so it wouldn't survive
/// introspection anyway. This strips the keyword for the apply and hands back
/// the index name so the compiler can re-mark the introspected index.
///
/// The declared name is what carries the concurrent intent across the
/// shadow-compile round trip. An unnamed `CREATE INDEX CONCURRENTLY ON ...`
/// therefore gets a name injected following Postgres's own convention
/// (`{table}_{columns}_idx`, expressions as `expr`), which also becomes the
/// index's name in the generated migration.
pub fn rewrite_create_index_concurrently(statement: &str) -> Result<Option<RewrittenCreateIndex>> {
    let Some(create) = typed_stmt::<ast::CreateIndex>(statement) else {
        return Ok(None);
    };
    let Some(concurrently) = create.concurrently_token() else {
        return Ok(None);
    };
    let start = usize::from(concurrently.text_range().start());
    let end = usize::from(concurrently.text_range().end());
    let stripped = |injected_name: &str| {
        format!(
            "{}{injected_name}{}",
            &statement[..start],
            statement[end..].trim_start()
        )
    };

    match create.index() {
        Some(index) => {
            let name_token = index
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| token.kind() == SyntaxKind::IDENT)
                .last()
                .ok_or_else(|| {
                    ShkiError::schema(format!("CREATE INDEX has an unreadable name: {statement}"))
                })?;
            Ok(Some(RewrittenCreateIndex {
                sql: stripped(""),
                index_name: fold_identifier(name_token.text()),
            }))
        }
        None => {
            // Unnamed index: inject a Postgres-convention name so the
            // concurrent intent can be tracked (and the generated migration
            // is explicit).
            let index_name = default_index_name(&create).ok_or_else(|| {
                ShkiError::schema(format!("CREATE INDEX names no table: {statement}"))
            })?;
            Ok(Some(RewrittenCreateIndex {
                sql: stripped(&format!("{index_name} ")),
                index_name,
            }))
        }
    }
}

/// Name an unnamed index the way Postgres would: `{table}_{columns}_idx`, with
/// non-column items (expressions, function calls) contributing `expr`.
///
/// ponytail: no collision dedup (Postgres would append `1`) — a duplicate name
/// fails the shadow apply loudly; name the index explicitly to resolve.
fn default_index_name(create: &ast::CreateIndex) -> Option<String> {
    let table = create
        .table_relation_name()
        .map(|relation| name_parts(relation.syntax()))?
        .pop()
        .map(|name| fold_identifier(&name))?;

    let parts = create
        .partition_item_list()
        .map(|list| {
            list.partition_items()
                .map(|item| match item.expr() {
                    Some(ast::Expr::NameRef(name)) => {
                        fold_identifier(name.syntax().text().to_string().trim())
                    }
                    _ => "expr".to_string(),
                })
                .collect::<Vec<_>>()
                .join("_")
        })
        .unwrap_or_default();

    let mut name = format!("{table}_{parts}_idx");
    // Postgres truncates identifiers to 63 bytes.
    if name.len() > 63 {
        let mut end = 63;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name.truncate(end);
    }
    Some(name)
}

pub fn join_sql_statements(statements: &[SqlStmt]) -> String {
    statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
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
            let rewritten = rewrite_create_index_concurrently(input)
                .expect("should parse")
                .expect("should detect CONCURRENTLY");
            assert_eq!(rewritten.sql, sql);
            assert_eq!(rewritten.index_name, name);
        }
    }

    #[test]
    fn plain_statements_are_not_rewritten() {
        for statement in [
            "CREATE INDEX users_email_idx ON users (email)",
            "CREATE INDEX ON users (email)",
            "CREATE TABLE concurrently (id int)",
            "DROP INDEX CONCURRENTLY users_email_idx",
        ] {
            assert!(
                rewrite_create_index_concurrently(statement)
                    .expect("should parse")
                    .is_none()
            );
        }
    }

    #[test]
    fn unnamed_concurrent_index_gets_a_postgres_convention_name() {
        let cases = [
            (
                "CREATE INDEX CONCURRENTLY ON hello (id)",
                "CREATE INDEX hello_id_idx ON hello (id)",
                "hello_id_idx",
            ),
            (
                "CREATE UNIQUE INDEX CONCURRENTLY ON app.users USING gin (email DESC, lower(name))",
                "CREATE UNIQUE INDEX users_email_expr_idx ON app.users USING gin (email DESC, lower(name))",
                "users_email_expr_idx",
            ),
        ];
        for (input, sql, name) in cases {
            let rewritten = rewrite_create_index_concurrently(input)
                .expect("should parse")
                .expect("should detect CONCURRENTLY");
            assert_eq!(rewritten.sql, sql);
            assert_eq!(rewritten.index_name, name);
        }
    }

    #[test]
    fn splits_statements_respecting_literals_and_dollar_quotes() {
        let statements = split_sql_statements(
            "CREATE TABLE t (name text DEFAULT 'a;b');\n\
             CREATE FUNCTION f() RETURNS int LANGUAGE sql AS $$ SELECT 1; $$;\n\
             SELECT 2",
        )
        .expect("should split");

        assert_eq!(statements.len(), 3);
        assert!(statements[0].contains("'a;b'"));
        assert!(statements[1].contains("SELECT 1;"));
        assert_eq!(statements[2], "SELECT 2");
    }

    #[test]
    fn unparseable_statements_pass_through_verbatim() {
        let statements = split_sql_statements("CREATE FLURB wibble (1);\nCREATE TABLE t (id int);")
            .expect("should split");

        assert!(statements.iter().any(|s| s.contains("FLURB")));
        assert!(statements.iter().any(|s| s.contains("CREATE TABLE t")));
    }
}
