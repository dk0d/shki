use serde::{Deserialize, Serialize};

use super::DataType;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeType {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<CompositeTypeColumn>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeTypeColumn {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Domain {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    pub base_type: DataType,

    #[serde(default)]
    pub not_null: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<DomainConstraint>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainConstraint {
    pub name: String,
    pub definition: String,
}
