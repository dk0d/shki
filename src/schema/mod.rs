use serde::{Deserialize, Serialize};

/// Database dialect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SchemaDialect {
    #[default]
    Postgres,
    Mysql,
    Sqlite,
}
