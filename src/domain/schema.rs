use serde::{Deserialize, Serialize};

use clap::ValueEnum;

/// Database dialect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SqlDialect {
    #[default]
    #[value(aliases = ["pg", "postgres"])]
    Postgres,
    Mysql,
    Sqlite,
}

impl std::fmt::Display for SqlDialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqlDialect::Postgres => write!(f, "postgresql"),
            SqlDialect::Mysql => write!(f, "mysql"),
            SqlDialect::Sqlite => write!(f, "sqlite"),
        }
    }
}
