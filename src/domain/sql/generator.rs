//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::models::table_id::TableId;
use crate::schema::SqlDialect;

/// SQL generator for a specific dialect
pub struct SqlGenerator {
    dialect: SqlDialect,
    breakpoints: bool,
}

impl SqlGenerator {
    /// Create a new SQL generator
    pub fn new(dialect: &SqlDialect) -> Self {
        Self {
            dialect: dialect.to_owned(),
            breakpoints: true,
        }
    }

    /// Set whether to include breakpoints
    pub fn with_breakpoints(mut self, breakpoints: bool) -> Self {
        self.breakpoints = breakpoints;
        self
    }

    // Helper methods

    fn quote_identifier(&self, name: impl Into<String>) -> String {
        let name: String = name.into();
        match self.dialect {
            SqlDialect::Postgres | SqlDialect::Sqlite => {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
            SqlDialect::Mysql => {
                format!("`{}`", name.replace('`', "``"))
            }
        }
    }

    pub fn qualified_table_name(&self, id: &TableId) -> String {
        match id.schema() {
            Some(s) => format!(
                "{}.{}",
                self.quote_identifier(s),
                self.quote_identifier(id.name.clone())
            ),
            None => self.quote_identifier(id.name.clone()),
        }
    }

    pub fn qualified_name(&self, name: impl Into<String>, schema: &Option<String>) -> String {
        match schema {
            Some(s) => format!(
                "{}.{}",
                self.quote_identifier(s),
                self.quote_identifier(name)
            ),
            None => self.quote_identifier(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers_per_dialect() {
        let table = TableId::new("odd\"name", Some("app\"schema".to_string()));

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
