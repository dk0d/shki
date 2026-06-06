use serde::{Deserialize, Serialize};

use crate::models::iden::Iden;

use super::{DataType, FunctionParameter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Procedure {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<FunctionParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aggregate {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<FunctionParameter>,
    pub return_type: DataType,
    pub state_type: DataType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_function: Option<Iden>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_function: Option<Iden>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowLevelSecurity {
    pub table: Iden,
    #[serde(default)]
    pub forced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowLevelSecurityPolicy {
    pub name: String,
    pub table: Iden,
    #[serde(default)]
    pub permissive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub using_expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_expression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionAttachment {
    pub parent: Iden,
    pub child: Iden,
    pub bound: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultPrivilege {
    pub owner_role: String,
    pub object_type: String,
    pub grantee: String,
    pub privilege_type: String,
    #[serde(default)]
    pub grantable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectPrivilege {
    pub object_type: String,
    pub object: Iden,
    pub grantee: String,
    pub privilege_type: String,
    #[serde(default)]
    pub grantable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnPrivilege {
    pub table: Iden,
    pub column: String,
    pub grantee: String,
    pub privilege_type: String,
    #[serde(default)]
    pub grantable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokedDefaultPrivilege {
    pub owner_role: String,
    pub object_type: String,
    pub grantee: String,
    pub privilege_type: String,
}
