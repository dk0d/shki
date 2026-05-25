use serde::{Deserialize, Serialize};

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
    pub data_type: String,
}
