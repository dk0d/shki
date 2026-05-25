use super::schema::SqlDialect;

pub fn ts_schema_template(dialect: &SqlDialect) -> String {
    "{}".to_string()
}

pub fn ts_schema_types_template(dialect: &SqlDialect) -> String {
    "{}".to_string()
}

pub fn ts_schema_virtual_module_template(dialect: &SqlDialect) -> String {
    "{}".to_string()
}
