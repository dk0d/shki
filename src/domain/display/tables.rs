use std::collections::{HashMap, HashSet};

use crate::Result;
use crate::config::Config;
use crate::migrate::manager::{MigrationManager, MigrationRow};
use colored::Colorize;
use tabled::Tabled;
use tabled::settings::Alignment;
use tabled::settings::object::Rows;
use tabled::{
    Table,
    settings::{Color, Style, object::Columns},
};

const DOWN_SYMBOL: &str = "↓";
const NO_DOWN_SYMBOL: &str = "-";

#[derive(Debug, Tabled)]
pub struct MigrationState {
    status: String,
    name: String,
    down: String,
    applied_at: String,
    checksum: String,
}

pub async fn display_migrations(manager: &MigrationManager, config: &Config) -> Result<()> {
    let all_migrations = manager.list_up_migrations()?;

    if all_migrations.is_empty() {
        println!("{}", "No migrations found".yellow());
        return Ok(());
    }

    let applied = if config.database_url.is_some() {
        Some(manager.try_get_applied_migrations().await?)
    } else {
        None
    };

    let applied_set: HashSet<&str> = applied
        .as_deref()
        .map(|rows| rows.iter().map(|m| m.name.as_str()).collect())
        .unwrap_or_default();

    let applied_by_name: HashMap<&str, &MigrationRow> = applied
        .as_deref()
        .map(|rows| rows.iter().map(|m| (m.name.as_str(), m)).collect())
        .unwrap_or_default();

    let migrations = all_migrations
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            let status = if applied_set.contains(name) {
                "✔".green()
            } else {
                "~".yellow()
            };

            let has_down = manager.has_down_migration(name);

            MigrationState {
                status: status.to_string(),
                name: name.bright_white().to_string(),
                checksum: applied_by_name
                    .get(name)
                    .and_then(|m| m.checksum.clone())
                    .unwrap_or_else(|| "-".dimmed().to_string()),
                down: if has_down {
                    format!(" {}", DOWN_SYMBOL.cyan())
                } else {
                    format!(" {}", NO_DOWN_SYMBOL.dimmed())
                },
                applied_at: applied_by_name
                    .get(name)
                    .map(|m| m.applied_at.clone())
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();

    let mut table = Table::new(&migrations);
    table
        .with(Style::psql())
        .modify(Rows::one(0), Color::BOLD)
        .modify(Columns::first(), Alignment::center())
        .modify(Columns::one(2), Alignment::center());

    println!("{}", table);

    Ok(())
}

pub fn display_migration_rows(migrations: &[MigrationRow]) {
    let mut table = Table::new(migrations);
    table
        .with(Style::psql())
        .modify(Columns::new(0..), Color::FG_BLUE);

    println!("{}", table);
}

pub fn display_config(config: &Config) {
    let mut builder = tabled::builder::Builder::default();
    builder.push_record(["Key".bold().to_string(), "Value".bold().to_string()]);

    let rows = match config_display_rows(config) {
        Ok(rows) => rows,
        _ => {
            println!("{}", "Failed to serialize config".red());
            return;
        }
    };

    for row in rows {
        match row.kind {
            ConfigDisplayRowKind::Section => {
                builder.push_record([format!("[{}]", row.key).bold().to_string(), String::new()])
            }
            ConfigDisplayRowKind::Value => {
                builder.push_record([row.key.dimmed().to_string(), row.value.clone()])
            }
        };
    }

    let mut table = builder.build();
    let table = table
        .with(Style::psql())
        .modify(Columns::first(), Color::BOLD);

    println!("{}", table);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigDisplayRow {
    key: String,
    value: String,
    kind: ConfigDisplayRowKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigDisplayRowKind {
    Section,
    Value,
}

fn config_display_rows(config: &Config) -> crate::Result<Vec<ConfigDisplayRow>> {
    let value = serde_yaml::to_value(config)?;
    let serde_yaml::Value::Mapping(map) = value else {
        return Ok(Vec::new());
    };

    let mut rows = Vec::new();
    let mut complex_sections = Vec::new();

    for (key, value) in map {
        let key = yaml_key(&key);
        if is_scalar_value(&value) {
            rows.push(value_row(key, yaml_value(&value)));
        } else {
            complex_sections.push((key, value));
        }
    }

    for (section, value) in complex_sections {
        rows.push(section_row(section.clone()));
        flatten_config_value(&section, &value, &mut rows);
    }

    Ok(rows)
}

fn flatten_config_value(prefix: &str, value: &serde_yaml::Value, rows: &mut Vec<ConfigDisplayRow>) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            if map.is_empty() {
                rows.push(value_row(prefix.to_string(), yaml_value(value)));
                return;
            }

            for (key, value) in map {
                let key = yaml_key(key);
                let next_prefix = format!("{}.{}", prefix, key);
                flatten_config_value(&next_prefix, value, rows);
            }
        }
        _ => rows.push(value_row(prefix.to_string(), yaml_value(value))),
    }
}

fn section_row(key: String) -> ConfigDisplayRow {
    ConfigDisplayRow {
        key,
        value: String::new(),
        kind: ConfigDisplayRowKind::Section,
    }
}

fn value_row(key: String, value: String) -> ConfigDisplayRow {
    ConfigDisplayRow {
        key,
        value,
        kind: ConfigDisplayRowKind::Value,
    }
}

fn yaml_key(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(value) => value.clone(),
        _ => "<complex>".to_string(),
    }
}

fn yaml_value(value: &serde_yaml::Value) -> String {
    let v = match value {
        serde_yaml::Value::Null => "-".to_string(),
        serde_yaml::Value::Bool(value) => value.to_string(),
        serde_yaml::Value::Number(value) => value.to_string(),
        serde_yaml::Value::String(value) if value.is_empty() => "-".to_string(),
        serde_yaml::Value::String(value) => value.to_string(),
        serde_yaml::Value::Sequence(values) => sequence_value(values),
        serde_yaml::Value::Mapping(map) if map.is_empty() => "{}".to_string(),
        serde_yaml::Value::Mapping(_) => "<complex>".to_string(),
        serde_yaml::Value::Tagged(tagged) => yaml_value(&tagged.value),
    };
    color_config_value(&v)
}

fn sequence_value(values: &[serde_yaml::Value]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }

    format!(
        "[{}]",
        values.iter().map(yaml_value).collect::<Vec<_>>().join(", ")
    )
}

fn is_scalar_value(value: &serde_yaml::Value) -> bool {
    matches!(
        value,
        serde_yaml::Value::Null
            | serde_yaml::Value::Bool(_)
            | serde_yaml::Value::Number(_)
            | serde_yaml::Value::String(_)
    )
}

fn color_config_value(value: &str) -> String {
    match value {
        "true" | "false" => value.yellow().to_string(),
        "null" => value.purple().to_string(),
        "-" | "{}" | "[]" => value.dimmed().to_string(),
        value if value.parse::<i64>().is_ok() => value.cyan().to_string(),
        value if value.starts_with('[') => value.to_string(),
        value => value.green().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::OutputMode;
    use crate::schema::SqlDialect;

    #[test]
    fn config_display_rows_flatten_complex_sections_at_the_end() {
        let mut config = Config {
            dialect: SqlDialect::Postgres,
            database_url: Some("postgres://localhost/app".to_string()),
            ..Config::default()
        };
        config.migrations.table = "schema_migrations".to_string();
        config.codegen.format = OutputMode::Module;
        config
            .codegen
            .type_overrides
            .insert("jsonb".to_string(), "JsonValue".to_string());

        let rows = config_display_rows(&config).expect("config rows should build");
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();

        let first_section = rows
            .iter()
            .position(|row| row.kind == ConfigDisplayRowKind::Section)
            .expect("complex sections should have headers");
        assert!(
            rows[..first_section]
                .iter()
                .all(|row| row.kind == ConfigDisplayRowKind::Value)
        );
        assert!(keys.contains(&"migrations"));
        assert!(keys.contains(&"migrations.table"));
        assert!(keys.contains(&"codegen"));
        assert!(keys.contains(&"codegen.format"));
        assert!(keys.contains(&"codegen.type_overrides.jsonb"));
        assert!(!rows.iter().any(|row| row.value == "<complex>"));
    }
}
