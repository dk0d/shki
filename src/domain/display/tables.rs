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

    // Try to get applied migrations if database URL is available
    let applied = if config.database_url.is_some() {
        let migrations = manager.get_applied_migrations().await?;
        Some(migrations)
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

            let checksum = "-".dimmed().to_string(); // Placeholder for checksum

            // Check if down migration exists
            let has_down = manager.has_down_migration(path);

            MigrationState {
                status: status.to_string(),
                name: name.bright_white().to_string(),
                checksum,
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

    // TODO: use verbose flag to show these types of things
    // Show legend
    // println!("   {} = down migration available", DOWN_SYMBOL.cyan());
    // println!("   {} = down migration not available", "x".red());

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

    let value = match serde_yaml::to_value(config) {
        Ok(serde_yaml::Value::Mapping(map)) => map,
        _ => {
            println!("{}", "Failed to serialize config".red());
            return;
        }
    };

    for (k, v) in value.iter() {
        let k = match k {
            serde_yaml::Value::String(s) => s.clone().dimmed().to_string(),
            _ => "<complex>".dimmed().to_string(),
        };
        let v = match v {
            serde_yaml::Value::String(s) => s.clone().green().to_string(),
            serde_yaml::Value::Number(n) => n.to_string().cyan().to_string(),
            serde_yaml::Value::Bool(b) => b.to_string().yellow().to_string(),
            _ => {
                // For complex types, just show a placeholder
                "<complex>".dimmed().to_string()
            }
        };
        builder.push_record([k, v]);
    }

    let mut table = builder.build();
    let table = table
        .with(Style::psql())
        .modify(Columns::first(), Color::BOLD);

    println!("{}", table);
}
