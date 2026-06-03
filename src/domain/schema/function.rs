use serde::{Deserialize, Serialize};

use super::{DataType, SqlDialect};

/// PostgreSQL function or procedure represented for catalog completeness.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Normalized identity signature used as the map key for overloads.
    pub signature: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<FunctionParameter>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<DataType>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Ordered routine parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionParameter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    pub data_type: DataType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<FunctionParameterMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionParameterMode {
    #[serde(rename = "IN")]
    In,
    #[serde(rename = "OUT")]
    Out,
    #[serde(rename = "INOUT")]
    InOut,
    #[serde(rename = "VARIADIC")]
    Variadic,
}

impl Function {
    pub fn parse_return_type(
        &mut self,
        return_type: impl Into<String>,
        dialect: &SqlDialect,
    ) -> &mut Self {
        self.return_type = Some(DataType::parse(return_type, dialect));
        self
    }
}

impl FunctionParameter {
    pub fn new(name: Option<String>, data_type: DataType) -> Self {
        Self {
            name,
            data_type,
            mode: None,
        }
    }

    pub fn parse(name: Option<String>, data_type: impl Into<String>, dialect: &SqlDialect) -> Self {
        Self::new(name, DataType::parse(data_type, dialect))
    }

    pub fn with_mode(mut self, mode: FunctionParameterMode) -> Self {
        self.mode = Some(mode);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_parameter_data_type_parses_postgres_normalized_string() {
        let parameter = FunctionParameter::parse(
            Some("email".to_string()),
            "character varying(255)",
            &SqlDialect::Postgres,
        )
        .with_mode(FunctionParameterMode::In);

        assert_eq!(parameter.data_type, DataType::VarChar { length: Some(255) });
    }

    #[test]
    fn function_parameter_data_type_parses_mysql_normalized_string() {
        let parameter = FunctionParameter::parse(None, "TINYINT(1)", &SqlDialect::Mysql);

        assert_eq!(parameter.data_type, DataType::Boolean);
    }

    #[test]
    fn function_return_type_parses_with_dialect() {
        let mut function = Function {
            name: "is_active".to_string(),
            schema: None,
            signature: "is_active(tinyint(1))".to_string(),
            parameters: Vec::new(),
            return_type: None,
            language: None,
            body: None,
        };

        function.parse_return_type("TINYINT(1)", &SqlDialect::Mysql);

        assert_eq!(function.return_type, Some(DataType::Boolean));
    }
}
