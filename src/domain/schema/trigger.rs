use serde::{Deserialize, Serialize};

use crate::models::iden::Iden;

/// PostgreSQL trigger represented for catalog completeness.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub name: String,
    pub table: Iden,
    pub function: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
}
