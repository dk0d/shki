use crate::config::Config;
use crate::migrate::manager::{
    MigrationRow, MigrationState, MigrationStatus, MigrationStatusEntry,
};
use owo_colors::OwoColorize;
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
struct MigrationDisplayRow {
    status: String,
    name: String,
    down: String,
    applied_at: String,
    checksum: String,
}

fn has_down_display(has_down: bool) -> String {
    if has_down {
        format!(" {}", DOWN_SYMBOL.cyan())
    } else {
        format!(" {}", NO_DOWN_SYMBOL.dimmed())
    }
}

pub fn display_migrations(status: &MigrationStatus) {
    if status.entries.is_empty() {
        println!("{}", "No migrations found".yellow());
        return;
    }

    let (missing, defined): (Vec<_>, Vec<_>) = status
        .entries
        .iter()
        .partition(|entry| entry.state == MigrationState::Missing);
    if !defined.is_empty() {
        display_migration_status_table(&defined);
    }

    if !missing.is_empty() {
        println!(
            "\n{}: DB has migrations that are not defined in local migrations",
            "**WARNING**".yellow(),
        );
        display_migration_status_table(&missing);
    }
}

fn display_migration_status_table(entries: &[&MigrationStatusEntry]) {
    let rows: Vec<_> = entries
        .iter()
        .map(|entry| MigrationDisplayRow {
            status: match entry.state {
                MigrationState::Applied => "✔ applied".green().to_string(),
                MigrationState::Pending => "~ pending".yellow().to_string(),
                MigrationState::Missing => "x missing".red().to_string(),
            },
            name: entry.name.bright_white().to_string(),
            down: has_down_display(entry.has_down),
            applied_at: entry
                .applied
                .as_ref()
                .map(|row| row.applied_at.clone())
                .unwrap_or_default(),
            checksum: entry
                .applied
                .as_ref()
                .and_then(|row| row.checksum.clone())
                .unwrap_or_else(|| "-".dimmed().to_string()),
        })
        .collect();
    let mut table = Table::new(rows);
    table
        .with(Style::psql())
        .modify(Rows::one(0), Color::BOLD)
        .modify(Columns::first(), Alignment::center())
        .modify(Columns::one(2), Alignment::center());
    println!("{}", table);
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
            common: crate::CommonArgs {
                dialect: Some(SqlDialect::Postgres),
                database_url: Some("postgres://localhost/app".to_string()),
                ..Default::default()
            },
            ..Config::default()
        };
        config.migrations.args.table = Some("schema_migrations".to_string());
        config.codegen.format = Some(OutputMode::Module);
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
