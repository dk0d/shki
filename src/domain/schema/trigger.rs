use serde::{Deserialize, Serialize};

use crate::models::iden::Iden;

/// PostgreSQL trigger represented for catalog completeness.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub name: String,
    pub table: Iden,
    pub function: Iden,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<TriggerEvent>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<TriggerTiming>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<TriggerOrientation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerEvent {
    #[serde(rename = "INSERT")]
    Insert,
    #[serde(rename = "UPDATE")]
    Update,
    #[serde(rename = "DELETE")]
    Delete,
    #[serde(rename = "TRUNCATE")]
    Truncate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerTiming {
    #[serde(rename = "BEFORE")]
    Before,
    #[serde(rename = "AFTER")]
    After,
    #[serde(rename = "INSTEAD OF")]
    InsteadOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerOrientation {
    #[serde(rename = "ROW")]
    Row,
    #[serde(rename = "STATEMENT")]
    Statement,
}
