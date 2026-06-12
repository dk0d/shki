//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::Result;
use crate::models::iden::Iden;
use crate::schema::{DefaultValue, SqlDialect};

use super::statements::{qualified_name, qualified_table_name, quote_identifier};

/// SQL generator for a specific dialect
pub struct SqlRenderer {
    dialect: SqlDialect,
    breakpoints: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStmtPart(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStmt {
    sql: String,
    object_type: SqlObjectType,
    operation: SqlOperation,
    identity: Option<Iden>,
    depends_on: Vec<Iden>,
    ordinal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SqlObjectType {
    Schema,
    DefaultPrivilege,
    Extension,
    Type,
    Function,
    Procedure,
    Aggregate,
    Sequence,
    Table,
    View,
    MaterializedView,
    Index,
    Trigger,
    Policy,
    Column,
    Rls,
    Privilege,
    ColumnPrivilege,
    RevokedDefaultPrivilege,
    #[default]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SqlOperation {
    Create,
    Alter,
    Drop,
    Rename,
    Comment,
    #[default]
    Raw,
}

impl AsRef<str> for SqlStmt {
    fn as_ref(&self) -> &str {
        &self.sql
    }
}

impl AsRef<str> for SqlStmtPart {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Single valid sql statement, ';' will be appended
#[derive(Debug)]
pub enum SqlOutput {
    Statement(SqlStmt),
    Script(Vec<SqlStmt>),
}

impl From<SqlStmtPart> for String {
    fn from(value: SqlStmtPart) -> Self {
        format!("{}", value)
    }
}

impl From<SqlStmt> for String {
    fn from(value: SqlStmt) -> Self {
        format!("{}", value)
    }
}

impl From<SqlOutput> for String {
    fn from(value: SqlOutput) -> Self {
        match value {
            SqlOutput::Statement(v) => String::from(v),
            SqlOutput::Script(values) => values
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }
}

impl std::fmt::Display for SqlStmtPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for SqlStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};", self.sql.trim_end_matches(';'))
    }
}

impl SqlStmtPart {
    pub fn as_sql(&self) -> &str {
        &self.0
    }
}

impl SqlStmt {
    pub fn as_sql(&self) -> &str {
        &self.sql
    }

    pub fn object_type(&self) -> SqlObjectType {
        self.object_type
    }

    pub fn operation(&self) -> SqlOperation {
        self.operation
    }

    pub fn identity(&self) -> Option<&Iden> {
        self.identity.as_ref()
    }

    pub fn depends_on(&self) -> &[Iden] {
        &self.depends_on
    }

    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub fn with_planning(
        mut self,
        object_type: SqlObjectType,
        operation: SqlOperation,
        ordinal: usize,
    ) -> Self {
        self.object_type = object_type;
        self.operation = operation;
        self.ordinal = ordinal;
        self
    }

    pub fn with_identity(mut self, identity: Iden) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn with_dependencies(mut self, depends_on: Vec<Iden>) -> Self {
        self.depends_on = depends_on;
        self
    }
}

impl std::fmt::Display for SqlOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string(None))
    }
}

impl SqlOutput {
    pub fn one(value: impl Into<SqlStmt>) -> Self {
        Self::Statement(value.into())
    }

    pub fn many(values: Vec<SqlStmt>) -> Self {
        Self::Script(values)
    }

    pub fn to_string(&self, sep: Option<&str>) -> String {
        match self {
            Self::Statement(value) => value.to_string(),
            Self::Script(parts) => parts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(sep.unwrap_or("\n")),
        }
    }

    pub fn parts(&self) -> Vec<SqlStmt> {
        match self {
            Self::Statement(val) => vec![val.clone()],
            Self::Script(parts) => parts.to_owned(),
        }
    }
}

impl From<String> for SqlStmtPart {
    fn from(v: String) -> Self {
        Self(v)
    }
}

impl From<&str> for SqlStmtPart {
    fn from(v: &str) -> Self {
        Self(v.to_string())
    }
}

impl From<String> for SqlStmt {
    fn from(v: String) -> Self {
        Self {
            sql: v.trim_end_matches(';').to_string(),
            object_type: SqlObjectType::Other,
            operation: SqlOperation::Raw,
            identity: None,
            depends_on: Vec::new(),
            ordinal: 0,
        }
    }
}

impl From<&str> for SqlStmt {
    fn from(v: &str) -> Self {
        Self {
            sql: v.trim_end_matches(';').to_string(),
            object_type: SqlObjectType::Other,
            operation: SqlOperation::Raw,
            identity: None,
            depends_on: Vec::new(),
            ordinal: 0,
        }
    }
}

impl From<SqlStmt> for SqlOutput {
    fn from(v: SqlStmt) -> Self {
        SqlOutput::Statement(v)
    }
}

impl From<Vec<SqlStmt>> for SqlOutput {
    fn from(mut v: Vec<SqlStmt>) -> Self {
        if v.len() == 1 {
            SqlOutput::Statement(v.remove(0))
        } else {
            SqlOutput::Script(v)
        }
    }
}

impl SqlRenderer {
    /// Create a new SQL generator
    pub fn new(dialect: &SqlDialect) -> Self {
        Self {
            dialect: dialect.to_owned(),
            breakpoints: None,
        }
    }

    /// Generate SQL for a vec of statements
    pub fn generate(&self, statements: &Vec<impl ToSql>) -> Result<Vec<SqlStmt>> {
        let mut sql_stmts: Vec<SqlStmt> = Vec::new();
        for stmt in statements {
            let sql = stmt.to_sql(&self.dialect)?;
            match sql {
                SqlOutput::Statement(stmt) => sql_stmts.push(stmt.clone()),
                SqlOutput::Script(stmts) => sql_stmts.extend(stmts),
            }
        }
        Ok(sql_stmts)
    }

    pub fn generate_string(&self, statements: &Vec<impl ToSql>) -> Result<String> {
        let sql_stmts: Vec<SqlStmt> = self.generate(statements)?;
        Ok(sql_stmts
            .into_iter()
            .map(String::from)
            .collect::<Vec<String>>()
            .join("\n"))
    }

    /// Set whether to include breakpoints
    pub fn with_breakpoints(mut self, breakpoints: &Option<String>) -> Self {
        self.breakpoints = breakpoints.clone();
        self
    }

    // Helper methods
    pub fn quote_identifier(&self, name: impl Into<String>) -> String {
        quote_identifier(&self.dialect, name)
    }

    pub fn statement(&self, sql: impl Into<String>) -> SqlStmt {
        SqlStmt::from(sql.into())
    }

    pub fn fragment(&self, sql: impl Into<String>) -> SqlStmtPart {
        SqlStmtPart::from(sql.into())
    }

    pub fn sql_literal(&self, value: &str) -> String {
        format!("'{}'", value.replace('\'', "''"))
    }

    pub fn default_value(&self, default: &DefaultValue) -> String {
        match default {
            DefaultValue::Literal(value) if is_unquoted_scalar_literal(value) => value.clone(),
            DefaultValue::Literal(value) => self.sql_literal(value),
            _ => default.to_string(),
        }
    }

    pub fn quoted_list<'a>(&self, values: impl IntoIterator<Item = &'a String>) -> String {
        values
            .into_iter()
            .map(|value| self.quote_identifier(value))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn qualified_table_name(&self, id: &Iden) -> String {
        qualified_table_name(&self.dialect, id)
    }

    pub fn qualified_name(&self, name: impl Into<String>, schema: &Option<String>) -> String {
        qualified_name(&self.dialect, name, schema)
    }
}

fn is_unquoted_scalar_literal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower == "true"
        || lower == "false"
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
}

pub trait ToSql {
    fn to_sql(&self, dialect: &SqlDialect) -> Result<SqlOutput>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers_per_dialect() {
        let table = Iden::new("odd\"name", Some("app\"schema".to_string()));
        assert_eq!(
            SqlRenderer::new(&SqlDialect::Postgres).qualified_table_name(&table),
            "\"app\"\"schema\".\"odd\"\"name\""
        );
        assert_eq!(
            SqlRenderer::new(&SqlDialect::Sqlite).qualified_name("users", &None),
            "\"users\""
        );
        assert_eq!(
            SqlRenderer::new(&SqlDialect::Mysql)
                .qualified_name("odd`name", &Some("app`schema".to_string())),
            "`app``schema`.`odd``name`"
        );
    }

    #[test]
    fn renders_default_literals_with_sql_string_escaping() {
        let renderer = SqlRenderer::new(&SqlDialect::Postgres);

        assert_eq!(
            renderer.default_value(&DefaultValue::Literal("default".to_string())),
            "'default'"
        );
        assert_eq!(
            renderer.default_value(&DefaultValue::Literal("owner's".to_string())),
            "'owner''s'"
        );
        assert_eq!(
            renderer.default_value(&DefaultValue::Literal("42".to_string())),
            "42"
        );
    }
}
