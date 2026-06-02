use serde::{Deserialize, Serialize};

/// PostgreSQL extension installed in a database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Extension {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            version: None,
        }
    }
}
