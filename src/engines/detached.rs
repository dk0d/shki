use crate::ShkiError;
use crate::models::iden::Iden;
use crate::schema::SqlDialect;

pub struct Detached {
    dialect: SqlDialect,
    table: Iden,
}

impl Detached {
    pub fn new(dialect: SqlDialect, table: Iden) -> Self {
        Self { dialect, table }
    }

    pub fn with_table(mut self, table: Iden) -> Self {
        self.table = table;
        self
    }

    pub fn table(&self) -> &Iden {
        &self.table
    }

    pub(crate) fn unavailable(&self) -> ShkiError {
        ShkiError::config(format!(
            "Database URL is required for {} operations",
            self.dialect
        ))
    }
}
