//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::models::entity_name::EntityName;
use crate::schema::SqlDialect;

use super::utils::{qualified_name, qualified_table_name, quote_identifier};

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
        quote_identifier(&self.dialect, name)
    }

    pub fn qualified_table_name(&self, id: &EntityName) -> String {
        qualified_table_name(&self.dialect, id)
    }

    pub fn qualified_name(&self, name: impl Into<String>, schema: &Option<String>) -> String {
        qualified_name(&self.dialect, name, schema)
    }
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
