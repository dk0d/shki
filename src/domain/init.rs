use owo_colors::OwoColorize;

use std::path::Path;

use crate::Result;
use crate::schema::SqlDialect;

const CONFIG_TEMPLATE: &str = include_str!("templates/shki.toml");
const POSTGRES_SCHEMA_TEMPLATE: &str = include_str!("templates/postgres_main.sql");
const CUSTOM_MIGRATION_SCHEMA_TEMPLATE: &str = include_str!("templates/custom_migration_main.sql");
const POSTGRES_LANGUAGE_SERVER_TEMPLATE: &str =
    include_str!("templates/postgres-language-server.jsonc.tmpl");
const INIT_SCHEMA_PATH: &str = "schema";
const INIT_TIMEOUT_SECONDS: u32 = 2;

/// Initialize a new shki project
pub async fn cmd_init(target_dir: &Path, dialect: Option<SqlDialect>) -> Result<()> {
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
    init_sql_project(target_dir, dialect.unwrap_or(SqlDialect::Postgres)).await?;

    Ok(())
}

async fn init_sql_project(target_dir: &Path, dialect: SqlDialect) -> Result<()> {
    let migrations_dir = target_dir.join("migrations");
    let schema_dir = target_dir.join("schema");

    std::fs::create_dir_all(migrations_dir.join("_meta"))?;
    std::fs::create_dir_all(&schema_dir)?;

    let config_path = target_dir.join("shki.toml");
    std::fs::write(&config_path, config_template(dialect))?;

    let schema_path = schema_dir.join("main.sql");
    if !schema_path.exists() {
        std::fs::write(&schema_path, schema_template(dialect))?;
    }

    let postgres_lsp_path = target_dir.join("postgres-language-server.jsonc");
    if dialect == SqlDialect::Postgres && !postgres_lsp_path.exists() {
        std::fs::write(
            &postgres_lsp_path,
            postgres_language_server_template(INIT_SCHEMA_PATH, INIT_TIMEOUT_SECONDS),
        )?;
    }

    println!("{}", "Initialized shki Declarative Schema project".green());
    println!();
    println!("  {}: {}", "Directory".cyan(), target_dir.display());
    println!();
    println!("  {}", "Created files:".cyan());
    println!(
        "    shki.toml                      - {}",
        "Project configuration".dimmed()
    );
    println!(
        "    schema/main.sql                - {}",
        "Declarative schema entrypoint".dimmed()
    );
    println!(
        "    migrations/_meta/              - {}",
        "Snapshot and Journal metadata".dimmed()
    );
    if dialect == SqlDialect::Postgres {
        println!(
            "    postgres-language-server.jsonc - {}",
            "Postgres Language Server configuration".dimmed()
        );
    }
    println!();
    println!("  {}", "Next steps:".cyan());
    println!("    1. Edit shki.toml to set database_url or export DATABASE_URL");
    println!("    2. Edit schema/main.sql to describe the intended database shape");
    println!("    3. Run {} to preview changes", "shki diff".yellow());
    println!(
        "    4. Run {} to create migration artifacts",
        "shki generate <name>".yellow()
    );

    Ok(())
}

fn config_template(dialect: SqlDialect) -> String {
    CONFIG_TEMPLATE.replace("{{dialect}}", config_dialect(dialect))
}

fn postgres_language_server_template(path: &str, timeout: u32) -> String {
    POSTGRES_LANGUAGE_SERVER_TEMPLATE
        .replace("{{schema}}", path)
        .replace("{{conn_timeout_secs}}", &timeout.to_string())
}

fn config_dialect(dialect: SqlDialect) -> &'static str {
    match dialect {
        SqlDialect::Postgres => "postgres",
        SqlDialect::Mysql => "mysql",
        SqlDialect::Sqlite => "sqlite",
    }
}

fn schema_template(dialect: SqlDialect) -> &'static str {
    match dialect {
        SqlDialect::Postgres => POSTGRES_SCHEMA_TEMPLATE,
        SqlDialect::Mysql | SqlDialect::Sqlite => CUSTOM_MIGRATION_SCHEMA_TEMPLATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommonArgs;
    use crate::config::Config;
    use tempfile::TempDir;

    #[tokio::test]
    async fn init_writes_config_into_target_directory() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let target_dir = temp_dir.path().join("project");

        cmd_init(&target_dir, Some(SqlDialect::Sqlite))
            .await
            .expect("init should succeed");

        assert!(target_dir.join("shki.toml").exists());
        assert!(!temp_dir.path().join("shki.toml").exists());
    }

    #[tokio::test]
    async fn init_creates_declarative_schema_project_layout() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let target_dir = temp_dir.path().join("project");

        cmd_init(&target_dir, Some(SqlDialect::Postgres))
            .await
            .expect("init should succeed");

        assert!(target_dir.join("shki.toml").exists());
        assert!(target_dir.join("schema/main.sql").exists());
        assert!(target_dir.join("migrations/_meta").is_dir());
        assert!(target_dir.join("postgres-language-server.jsonc").exists());
        assert!(!target_dir.join("schema/index.ts").exists());
        assert!(!target_dir.join("schema/.shki/schema.ts").exists());

        let pgls_config =
            std::fs::read_to_string(target_dir.join("postgres-language-server.jsonc"))
                .expect("postgres language server config should be readable");
        assert!(pgls_config.contains(r#""schema""#));
        assert!(pgls_config.contains("\"connTimeoutSecs\": 2"));

        let config = Config::load(&target_dir.join("shki.toml"), &CommonArgs::default())
            .expect("generated config should load");
        let target_dir = std::fs::canonicalize(target_dir).unwrap();
        assert_eq!(config.schema_path(), target_dir.join("schema"));
        assert_eq!(config.out_dir(), target_dir.join("migrations"));
    }

    #[tokio::test]
    async fn init_skips_postgres_language_server_config_for_non_postgres() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let target_dir = temp_dir.path().join("project");

        cmd_init(&target_dir, Some(SqlDialect::Sqlite))
            .await
            .expect("init should succeed");

        assert!(!target_dir.join("postgres-language-server.jsonc").exists());
    }
}
