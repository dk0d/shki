use crate::models::entity_name::EntityName;
use crate::schema::SqlDialect;

pub fn quote_identifier(dialect: &SqlDialect, name: impl Into<String>) -> String {
    let name: String = name.into();
    match dialect {
        SqlDialect::Postgres | SqlDialect::Sqlite => {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
        SqlDialect::Mysql => {
            format!("`{}`", name.replace('`', "``"))
        }
    }
}

pub fn qualified_table_name(dialect: &SqlDialect, id: &EntityName) -> String {
    match id.schema() {
        Some(s) => format!(
            "{}.{}",
            quote_identifier(dialect, s),
            quote_identifier(dialect, id.name.clone())
        ),
        None => quote_identifier(dialect, id.name.clone()),
    }
}

pub fn qualified_name(
    dialect: &SqlDialect,
    name: impl Into<String>,
    schema: &Option<String>,
) -> String {
    match schema {
        Some(s) => format!(
            "{}.{}",
            quote_identifier(dialect, s),
            quote_identifier(dialect, name)
        ),
        None => quote_identifier(dialect, name),
    }
}
