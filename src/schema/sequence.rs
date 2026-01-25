use serde::{Deserialize, Serialize};

/// Sequence definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub schema: Option<String>,
    #[serde(default = "default_increment")]
    pub increment: i64,
    #[serde(default = "default_min_value")]
    pub min_value: i64,
    pub max_value: Option<i64>,
    #[serde(default = "default_start")]
    pub start: i64,
    #[serde(default)]
    pub cache: i64,
    #[serde(default)]
    pub cycle: bool,
}

fn default_increment() -> i64 {
    1
}
fn default_min_value() -> i64 {
    1
}
fn default_start() -> i64 {
    1
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            name: String::new(),
            schema: None,
            increment: 1,
            min_value: 1,
            max_value: None,
            start: 1,
            cache: 1,
            cycle: false,
        }
    }
}
