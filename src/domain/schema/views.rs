use serde::{Deserialize, Serialize};

use super::{DataType, SqlDialect};

/// View definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct View {
    pub name: String,
    pub schema: Option<String>,
    pub definition: String,
    #[serde(default)]
    pub materialized: bool,
    pub columns: Vec<ViewColumn>,
}

/// View column
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ViewColumn {
    pub name: String,
    pub data_type: DataType,
}

impl ViewColumn {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
        }
    }

    pub fn parse(
        name: impl Into<String>,
        data_type: impl Into<String>,
        dialect: &SqlDialect,
    ) -> Self {
        Self::new(name, DataType::parse(data_type, dialect))
    }
}
