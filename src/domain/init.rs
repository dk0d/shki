use colored::Colorize;

use std::path::Path;

use crate::Result;
use crate::config::{Config, SchemaMode};
use crate::schema::SqlDialect;
use crate::templates::{
    ts_schema_template, ts_schema_types_template, ts_schema_virtual_module_template,
};
use crate::utils::resolve_path;

/// Initialize a new shki project
pub async fn cmd_init(
    target_dir: &Path,
    dialect: Option<SqlDialect>,
    mode: Option<SchemaMode>,
) -> Result<()> {
    // Create target directory if it doesn't exist
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    let config_path = target_dir.join("shki.toml");

    if config_path.exists() {
        println!(
            "\n{} already exists in {}",
            "shki.toml".yellow(),
            target_dir.display()
        );
        return Ok(());
    }
    let dialect = dialect.unwrap_or(SqlDialect::Postgres);
    let config = Config {
        dialect,
        ..Config::default()
    };

    config.save(&config_path)?;

    match mode.unwrap_or_default() {
        SchemaMode::Typescript => {
            init_ts_project(target_dir, &config).await?;
        }
        SchemaMode::Sql => {
            // only init the default config and exit
            init_sql_project(target_dir, &config).await?;
        }
    }

    Ok(())
}

async fn init_sql_project(target_dir: &Path, _config: &Config) -> Result<()> {
    println!("{}", "Initialized shki project (SQL)".green());
    println!();
    println!("  {}: {}", "Directory".cyan(), target_dir.display());
    println!();
    println!("  {}", "Created files:".cyan());
    println!(
        "    shki.toml        - {}",
        "project configuration".dimmed()
    );
    println!();
    println!("  {}", "Next steps:".cyan());
    println!("    1. Edit shki.toml to configure your project");
    println!("    2. Create a SQL schema file (e.g. schema.sql)");
    println!(
        "    3. Run {} to create migrations",
        "shki generate --schema schema.sql".yellow()
    );
    Ok(())
}

/// Initialize a Typescript-based shki project
async fn init_ts_project(target_dir: &Path, config: &Config) -> Result<()> {
    let dialect = &config.dialect;
    let migrations_dir = resolve_path(
        Some(target_dir.to_path_buf()),
        config.migrations_dir.clone(),
    );
    let schema_dir = resolve_path(Some(target_dir.to_path_buf()), config.schema.clone());
    let shki_dir = schema_dir.join(".shki");
    let schema_file = schema_dir.join("index.ts");

    // Create directories
    std::fs::create_dir_all(&migrations_dir)?;
    std::fs::create_dir_all(migrations_dir.join("_meta"))?;
    std::fs::create_dir_all(&shki_dir)?;

    // Create schema.lua with starter template
    if !schema_file.exists() {
        std::fs::write(&schema_file, ts_schema_template(dialect))?;
    }

    // Create Typescript type definitions
    let shki_types_path = shki_dir.join("schema.d.ts");
    if !shki_types_path.exists() {
        std::fs::write(&shki_types_path, ts_schema_types_template(dialect))?;
    }
    let shki_virtual_module_path = shki_dir.join("schema.ts");
    if !shki_virtual_module_path.exists() {
        std::fs::write(
            &shki_virtual_module_path,
            ts_schema_virtual_module_template(dialect),
        )?;
    }

    println!("{}", "Initialized shki project (TS)".green());
    println!();
    println!("  {}: {}", "Directory".cyan(), target_dir.display());
    println!();
    println!("  {}", "Created files:".cyan());
    println!(".");
    #[rustfmt::skip]
    println!("├── {}", target_dir.display());
    #[rustfmt::skip]
    println!("│   ├── migrations/    - {}", "Migrations directory".dimmed());
    #[rustfmt::skip]
    println!("│   ├── schema/           - {}", "Supporting lua files".dimmed());
    #[rustfmt::skip]
    println!("│   │   ├── .shki/schema.ts    - {}", "Main entrypoint for schema".dimmed());
    #[rustfmt::skip]
    println!("│   │   ├── .shki/schema.d.ts    - {}", "Main entrypoint for schema".dimmed());
    #[rustfmt::skip]
    println!("│   │   ├── index.ts    - {}", "Main entrypoint for schema".dimmed());
    #[rustfmt::skip]
    println!("│   │   └── tsconfig.ts    - {}", "Main entrypoint for schema".dimmed());
    #[rustfmt::skip]
    println!("│   └── shki.toml          - {}", "project configuration".dimmed());
    println!("  {}", "Next steps:".cyan());
    println!("    1. Edit schema/index.ts to define your schema");

    #[rustfmt::skip]
    println!("    2. Run {} to create migrations",
             "shki generate --schema schema/init.lua".yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn init_writes_config_into_target_directory() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let target_dir = temp_dir.path().join("project");

        cmd_init(&target_dir, Some(SqlDialect::Sqlite), Some(SchemaMode::Sql))
            .await
            .expect("init should succeed");

        assert!(target_dir.join("shki.toml").exists());
        assert!(!temp_dir.path().join("shki.toml").exists());
    }
}
