use colored::Colorize;

use std::path::Path;

use crate::Result;
// use crate::cli::templates::lua_schema_template;
use crate::config::{Config, SchemaMode};
// use crate::constants::{SELENE_CONFIG, luarc_config, selene_shki_std};
use crate::schema::SqlDialect;

// use super::SchemaLanguage;
// use crate::cli::constants::luacats_shki_types;

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

    let config_path = std::env::current_dir()
        .unwrap_or(target_dir.into())
        .join("shki.toml");

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
        root: target_dir.into(),
        dialect,
        ..Config::default()
    };

    config.save(&config_path)?;

    match mode.unwrap_or_default() {
        SchemaMode::Lua => {
            init_lua_project(target_dir, &config).await?;
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

/// Initialize a Lua-based shki project
async fn init_lua_project(target_dir: &Path, _config: &Config) -> Result<()> {
    // let dialect = config.dialect;
    // let lua_dir = target_dir.join("lua");
    // let types_dir = target_dir.join(".luacats");
    let migrations_dir = target_dir.join("migrations");
    // let schema_file = target_dir.join("init.lua");

    // Create directories
    std::fs::create_dir_all(&migrations_dir)?;
    std::fs::create_dir_all(migrations_dir.join("_meta"))?;
    // std::fs::create_dir_all(&lua_dir)?;
    // std::fs::create_dir_all(&types_dir)?;

    // Create schema.lua with starter template
    // if !schema_file.exists() {
    //     std::fs::write(&schema_file, lua_schema_template(dialect))?;
    // }

    // Create LuaCATS type definitions (for lua-language-server)
    // let shki_types_path = types_dir.join("shki.lua");
    // std::fs::write(&shki_types_path, luacats_shki_types())?;

    // Create .luarc.json for Lua Language Server
    // let luarc_path = target_dir.join(".luarc.json");
    // std::fs::write(&luarc_path, luarc_config())?;

    // Create Selene linter configuration
    // let selene_toml_path = target_dir.join("selene.toml");
    // std::fs::write(&selene_toml_path, SELENE_CONFIG)?;

    // Create Selene standard library definition for shki
    // let selene_std_path = target_dir.join("shki.yml");
    // std::fs::write(&selene_std_path, selene_shki_std())?;

    println!("{}", "Initialized shki project (Lua)".green());
    println!();
    println!("  {}: {}", "Directory".cyan(), target_dir.display());
    println!();
    println!("  {}", "Created files:".cyan());
    println!(".");
    // #[rustfmt::skip]
    // println!("├── {}", target_dir.display());
    // #[rustfmt::skip]
    // println!("│   ├── lua/           - {}", "Supporting lua files".dimmed());
    // #[rustfmt::skip]
    // println!("│   ├── migrations/    - {}", "Migrations directory".dimmed());
    // #[rustfmt::skip]
    // println!("│   ├── .luacats/      - {}", "LuaCATS type definitions".dimmed());
    // #[rustfmt::skip]
    // println!("│   ├── .luarc.json    - {}", "Lua Language Server config".dimmed());
    // #[rustfmt::skip]
    // println!("│   ├── init.lua       - {}", "Schema definition".dimmed());
    // #[rustfmt::skip]
    // println!("│   ├── selene.toml    - {}", "Selene linter config".dimmed());
    // #[rustfmt::skip]
    // println!("│   └── shki.yml       - {}", "Selene standard library".dimmed());
    #[rustfmt::skip]
    println!("└── shki.toml          - {}", "project configuration".dimmed());
    println!();
    // println!("  {}", "IDE/Linter Support:".cyan());
    // println!("    - lua-language-server: autocomplete, type checking, hover docs");
    // println!("    - selene: linting with shki globals recognized");
    // println!();
    // println!("init.lua must return a schema definition.");
    // println!();
    // println!("  {}", "Next steps:".cyan());
    // println!("    1. Edit schema/init.lua to define your schema");

    #[rustfmt::skip]
    println!("    2. Run {} to create migrations",
             "shki generate --schema schema/init.lua".yellow()
    );

    Ok(())
}
