//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::Result;
use crate::models::entity_name::EntityName;
use crate::schema::SqlDialect;

use super::statements::{qualified_name, qualified_table_name, quote_identifier};

/// SQL generator for a specific dialect
pub struct SqlGenerator {
    dialect: SqlDialect,
    breakpoints: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStmtPart(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStmt(String);

impl AsRef<str> for SqlStmt {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SqlStmtPart {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Single valid sql statement, ';' will be appended
pub enum SqlOutput {
    Statement(SqlStmt),
    Script(Vec<SqlStmt>),
}

impl std::fmt::Display for SqlStmtPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for SqlStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};", self.0.trim_end_matches(';'))
    }
}

impl SqlStmtPart {
    pub fn as_sql(&self) -> &str {
        &self.0
    }
}

impl SqlStmt {
    pub fn as_sql(&self) -> &str {
        &self.0
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
        Self(v.trim_end_matches(';').to_string())
    }
}

impl From<&str> for SqlStmt {
    fn from(v: &str) -> Self {
        Self(v.trim_end_matches(';').to_string())
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

impl SqlGenerator {
    /// Create a new SQL generator
    pub fn new(dialect: &SqlDialect) -> Self {
        Self {
            dialect: dialect.to_owned(),
            breakpoints: None,
        }
    }

    /// Generate SQL for a vec of statements
    pub fn generate(&self, statements: &Vec<impl ToSql>) -> Result<Vec<SqlOutput>> {
        let mut sql_stmts = Vec::new();
        for stmt in statements {
            let sql = stmt.to_sql(&self.dialect)?;
            sql_stmts.push(sql);
        }
        Ok(sql_stmts)
    }

    /// Set whether to include breakpoints
    pub fn with_breakpoints(mut self, breakpoints: &Option<String>) -> Self {
        self.breakpoints = breakpoints.clone();
        self
    }

    // Helper methods
    fn quote_identifier(&self, name: impl Into<String>) -> String {
        quote_identifier(&self.dialect, name)
    }

    pub fn qualified_table_name(&self, id: &EntityName) -> String {
        qualified_table_name(&self.dialect, id)
    }

    pub fn qualified_name(&self, name: impl Into<String>, schema: &Option<String>) -> String {
        qualified_name(&self.dialect, name, schema)
    }
}

pub trait ToSql {
    fn to_sql(&self, dialect: &SqlDialect) -> Result<SqlOutput>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers_per_dialect() {
        let table = EntityName::new("odd\"name", Some("app\"schema".to_string()));
        assert_eq!(
            SqlGenerator::new(&SqlDialect::Postgres).qualified_table_name(&table),
            "\"app\"\"schema\".\"odd\"\"name\""
        );
        assert_eq!(
            SqlGenerator::new(&SqlDialect::Sqlite).qualified_name("users", &None),
            "\"users\""
        );
        assert_eq!(
            SqlGenerator::new(&SqlDialect::Mysql)
                .qualified_name("odd`name", &Some("app`schema".to_string())),
            "`app``schema`.`odd``name`"
        );
    }
}
