use serde::{Deserialize, Serialize};

/// View definition
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct ViewColumn {
    pub name: String,
    pub data_type: String,
}
