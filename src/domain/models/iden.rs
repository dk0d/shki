use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ShkiError;

/// Identifier for a table, combining name and optional schema
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Iden {
    pub name: String,
    pub schema: Option<String>,
}

impl Serialize for Iden {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl<'de> Deserialize<'de> for Iden {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IdenVisitor;

        impl<'de> Visitor<'de> for IdenVisitor {
            type Value = Iden;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Iden")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Iden::parse(v).map_err(|e| E::custom(format!("{e}")))
            }
        }

        deserializer.deserialize_str(IdenVisitor)
    }
}

impl std::fmt::Display for Iden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.schema.as_ref() {
            Some(schema) => {
                write!(f, "{}.{}", schema, self.name)
            }
            None => {
                write!(f, "{}", self.name)
            }
        }
    }
}

impl From<(String, Option<String>)> for Iden {
    fn from((name, schema): (String, Option<String>)) -> Self {
        Self { name, schema }
    }
}

impl From<String> for Iden {
    fn from(name: String) -> Self {
        Self::unsafe_parse(&name)
    }
}

impl From<&str> for Iden {
    fn from(name: &str) -> Self {
        Self::unsafe_parse(name)
    }
}

impl Iden {
    pub fn new(name: impl Into<String>, schema: Option<String>) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }

    pub fn unsafe_parse(value: &str) -> Self {
        let parsed = Self::parse(value);
        match parsed {
            Ok(iden) => iden,
            Err(_) => Iden {
                name: value.to_owned(),
                schema: None,
            },
        }
    }

    pub fn parse(value: &str) -> crate::Result<Self> {
        let parts: Vec<&str> = value.split('.').collect();
        const EXPECTED: &[&str] = &["name", "schema.name"];
        let (schema, name) = match parts.as_slice() {
            [name] if !name.is_empty() => (None, name.to_string()),
            [schema, name] if !schema.is_empty() && !name.is_empty() => {
                (Some(schema.to_string()), name.to_string())
            }
            _ => {
                return Err(ShkiError::Parse(format!(
                    "invalid Iden `{value}`, expected {}",
                    EXPECTED.join(" or ")
                )));
            }
        };

        Ok(Iden { name, schema })
    }

    fn with_name(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: self.schema.clone(),
        }
    }

    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn column(&self, name: impl Into<String>) -> ColumnId {
        ColumnId::new(self.clone(), name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnId {
    table: Iden,
    name: String,
}

impl ColumnId {
    pub fn new(table: Iden, name: impl Into<String>) -> Self {
        Self {
            table,
            name: name.into(),
        }
    }

    pub fn table(&self) -> &Iden {
        &self.table
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    pub fn test_deserialize_iden_name() {
        let iden: Iden = serde_json::from_str("\"users\"").expect("deserialize");

        assert_eq!(iden.name, "users");
        assert_eq!(iden.schema, None);
    }

    #[test]
    pub fn test_deserialize_iden_schema_name() {
        let iden: Iden = serde_json::from_str("\"app.users\"").expect("deserialize");

        assert_eq!(iden.name, "users");
        assert_eq!(iden.schema, Some("app".to_string()));
    }

    #[test]
    pub fn test_deserialize_iden_rejects_invalid_input() {
        for input in ["", ".users", "app.", "app.users.extra"] {
            let result = serde_json::from_str::<Iden>(&format!("\"{input}\""));

            assert!(result.is_err(), "expected {input:?} to be rejected");
        }
    }
}
