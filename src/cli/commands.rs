use colored::Colorize;
use std::path::Path;

use crate::Result;
use crate::cli::templates::lua_schema_template;
use crate::config::Config;
use crate::schema::SchemaDialect;

use super::{Cli, SchemaLanguage};
use crate::cli::constants::LUACATS_SHKI_TYPES;

/// Initialize a new shki project
async fn cmd_init(
    target_dir: &Path,
    dialect: Option<SchemaDialect>,
    language: SchemaLanguage,
) -> Result<()> {
    // Create target directory if it doesn't exist
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    let config_path = target_dir.join("shki.toml");

    if config_path.exists() {
        println!(
            "{} {}",
            "shki.toml already exists in".yellow(),
            target_dir.display()
        );
        return Ok(());
    }

    let dialect = dialect.unwrap_or(SchemaDialect::Postgres);

    match language {
        // SchemaLanguage::Rust => init_rust_project(target_dir, dialect).await,
        SchemaLanguage::Lua => init_lua_project(target_dir, dialect).await,
    }
}

/// Initialize a Lua-based shki project
async fn init_lua_project(target_dir: &Path, dialect: SchemaDialect) -> Result<()> {
    todo!();
}

pub async fn run(cli: Cli) -> Result<()> {
    // Load config
    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        Config::default()
    };
    todo!();
}
