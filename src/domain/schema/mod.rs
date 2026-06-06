pub mod column;
pub use column::*;
pub mod types;
pub use types::*;
pub mod user_defined_type;
pub use user_defined_type::*;
pub mod postgres_catalog;
pub use postgres_catalog::*;
pub mod table;
pub use table::*;
pub mod constraint;
pub use constraint::*;
pub mod index;
pub use index::*;
pub mod sequence;
pub use sequence::*;
pub mod views;
pub use views::*;
pub mod function;
pub use function::*;
pub mod extension;
pub use extension::*;

pub mod trigger;
pub use trigger::*;

pub mod catalog;
pub use catalog::*;

// pub mod builders;
// pub use builders::*;

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
