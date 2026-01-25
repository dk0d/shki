use colored::Colorize;
use std::path::Path;

use crate::cli::templates::lua_schema_template;
use crate::config::Config;
use crate::constants::{LUARC_CONFIG, SELENE_CONFIG, SELENE_SHKI_STD};
use crate::schema::SchemaDialect;
use crate::{Commands, Result};

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
    let config_path = target_dir.join("shki.toml");
    let schema_dir = target_dir.join("schema");
    let types_dir = target_dir.join(".luacats");
    let migrations_dir = target_dir.join("migrations");
    let schema_file = schema_dir.join("init.lua");

    let config = Config {
        dialect,
        schema: vec!["./schema/init.lua".to_string()],
        ..Config::default()
    };

    config.save(&config_path)?;

    // Create directories
    std::fs::create_dir_all(&migrations_dir)?;
    std::fs::create_dir_all(migrations_dir.join("_meta"))?;
    std::fs::create_dir_all(&schema_dir)?;
    std::fs::create_dir_all(&types_dir)?;

    // Create schema.lua with starter template
    if !schema_file.exists() {
        std::fs::write(&schema_file, lua_schema_template(dialect))?;
    }

    // Create LuaCATS type definitions (for lua-language-server)
    let shki_types_path = types_dir.join("shki.lua");
    std::fs::write(&shki_types_path, LUACATS_SHKI_TYPES)?;

    // Create .luarc.json for Lua Language Server
    let luarc_path = target_dir.join(".luarc.json");
    std::fs::write(&luarc_path, LUARC_CONFIG)?;

    // Create Selene linter configuration
    let selene_toml_path = target_dir.join(".selene.toml");
    std::fs::write(&selene_toml_path, SELENE_CONFIG)?;

    // Create Selene standard library definition for shki
    let selene_std_path = target_dir.join("shki.yml");
    std::fs::write(&selene_std_path, SELENE_SHKI_STD)?;

    println!("{}", "Initialized shki project (Lua)".green());
    println!();
    println!("  {}: {}", "Directory".cyan(), target_dir.display());
    println!();
    println!("  {}", "Created files:".cyan());
    #[rustfmt::skip]
    println!("    migrations/      - {}", "Migrations directory".dimmed());
    #[rustfmt::skip]
    println!("    schema/init.lua  - {}", "Schema definition".dimmed());
    #[rustfmt::skip]
    println!("    .luacats         - {}", "LuaCATS type definitions".dimmed());
    #[rustfmt::skip]
    println!("    .luarc.json      - {}", "Lua Language Server config".dimmed());
    #[rustfmt::skip]
    println!("    selene.toml      - {}", "Selene linter config".dimmed());
    #[rustfmt::skip]
    println!("    shki.yml         - {}", "Selene standard library".dimmed());
    #[rustfmt::skip]
    println!("    shki.toml        - {}", "project configuration".dimmed());
    println!();
    println!("  {}", "IDE/Linter Support:".cyan());
    println!("    - lua-language-server: autocomplete, type checking, hover docs");
    println!("    - selene: linting with shki globals recognized");
    println!();
    println!("  {}", "Next steps:".cyan());
    println!("    1. Edit schema/init.lua to define your schema");
    #[rustfmt::skip]
    println!("    2. Run {} to create migrations",
             "shki generate --schema schema/init.lua".yellow()
    );

    Ok(())
}

pub async fn run(cli: Cli) -> Result<()> {
    // Load config
    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        Config::default()
    };

    // Override with CLI args
    if let Some(dialect) = cli.dialect {
        config.dialect = dialect.into();
    }
    if let Some(url) = cli.database_url {
        config.database_url = Some(url);
    }
    if let Some(out) = cli.out {
        config.out = out;
    }

    config.verbose = cli.verbose;

    match cli.command {
        Commands::Init {
            path,
            dialect,
            language,
        } => cmd_init(&path, dialect.map(Into::into), language).await,
        _ => todo!(),
    }
}
