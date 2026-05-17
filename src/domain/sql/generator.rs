//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use std::ops::Deref;

use crate::Result;
use crate::models::entity_name::EntityName;
use crate::schema::SqlDialect;

use super::statements::{qualified_name, qualified_table_name, quote_identifier};

/// SQL generator for a specific dialect
pub struct SqlGenerator {
    dialect: SqlDialect,
    breakpoints: Option<String>,
}

pub enum SqlStmt {
    One(String),
    Many(Vec<String>),
}

impl std::fmt::Display for SqlStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string(None))
    }
}

impl SqlStmt {
    pub fn one(value: String) -> Self {
        Self::One(value)
    }

    pub fn many(values: Vec<String>) -> Self {
        Self::Many(values)
    }

    pub fn to_string(&self, sep: Option<&str>) -> String {
        match self {
            Self::One(value) => value.clone(),
            Self::Many(parts) => parts.join(sep.unwrap_or(" ")),
        }
    }

    pub fn parts(&self) -> Vec<String> {
        match self {
            Self::One(val) => vec![val.clone()],
            Self::Many(parts) => parts.to_owned(),
        }
    }
}

impl From<String> for SqlStmt {
    fn from(v: String) -> Self {
        SqlStmt::One(v)
    }
}
impl From<Vec<String>> for SqlStmt {
    fn from(v: Vec<String>) -> Self {
        SqlStmt::Many(v)
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
    pub fn generate(&self, statements: &Vec<impl ToSql>) -> Result<Vec<SqlStmt>> {
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
    fn to_sql(&self, dialect: &SqlDialect) -> Result<SqlStmt>;
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
