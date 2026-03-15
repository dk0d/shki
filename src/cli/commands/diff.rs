use super::introspect::introspect_db;
use crate::{Config, MigrationManager, Snapshot, diff::diff_snapshots};

use colored::Colorize;

use crate::{Result, SqlGenerator};

fn load_desired_snapshot(
    config: &Config,
    schema_path: Option<&std::path::Path>,
) -> Result<Snapshot> {
    if let Some(path) = schema_path {
        let resolved_path = config.resolve_path(path);
        Snapshot::from_path(&resolved_path)
    } else {
        Snapshot::from_config(config)
    }
}

fn load_base_snapshot(config: &Config) -> Result<Snapshot> {
    let migration_manager = MigrationManager::new(config.out_dir(), config.dialect);
    migration_manager
        .load_latest_snapshot()?
        .map(Ok)
        .unwrap_or_else(|| Ok(Snapshot::new(config.dialect)))
}

/// Show the diff between schema and database or local snapshots
pub async fn cmd_diff(
    config: &Config,
    schema_path: Option<&std::path::Path>,
    show_sql: bool,
) -> Result<()> {
    let desired_snapshot = load_desired_snapshot(config, schema_path)?;

    let base_snapshot = if let Some(db_url) = config.database_url.as_deref() {
        println!("{}", "Introspecting database...".yellow());
        println!("URL: {}", db_url.bright_green());
        introspect_db(config).await?
    } else {
        println!(
            "{}",
            "No database URL found; diffing schema against the latest generated snapshot."
                .bright_yellow()
        );
        load_base_snapshot(config)?
    };

    let diff = diff_snapshots(&base_snapshot, &desired_snapshot)?;

    if diff.is_empty() {
        println!("{}", "No differences found".green());
        return Ok(());
    }

    println!("\n{}", "Differences:".yellow());
    println!("{}", diff.summary());

    if show_sql {
        let generator = SqlGenerator::new(config.dialect);
        let sql = generator.generate_sql(&diff)?;
        println!("\n{}", "SQL:".cyan());
        println!("{}", sql);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_base_snapshot, load_desired_snapshot};
    use crate::{Config, SchemaDialect, Snapshot};

    #[test]
    fn diff_uses_configured_lua_schema_when_no_schema_path_is_passed() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let lua_path = temp_dir.path().join("init.lua");
        std::fs::write(
            &lua_path,
            r#"
local schema = pg.schema("public")
local Table = TableBuilder
local Col = ColumnBuilder

schema:table(
    Table.new("users")
        :column(Col.integer("id"):primary_key())
)

return schema
"#,
        )
        .expect("failed to write init.lua");

        let config = Config {
            root: temp_dir.path().to_path_buf(),
            schema: "init.lua".to_string(),
            out: temp_dir.path().join("migrations"),
            dialect: SchemaDialect::Postgres,
            ..Config::default()
        };

        let snapshot =
            load_desired_snapshot(&config, None).expect("failed to load desired snapshot");

        assert!(snapshot.tables.contains_key("users"));
    }

    #[test]
    fn diff_falls_back_to_latest_generated_snapshot_without_database_url() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let out_dir = temp_dir.path().join("migrations");

        let mut snapshot = Snapshot::new(SchemaDialect::Postgres);
        snapshot.tables.insert(
            "users".to_string(),
            crate::snapshot::TableSnapshot {
                name: "users".to_string(),
                schema: None,
                columns: Default::default(),
                constraints: Vec::new(),
                indexes: Default::default(),
                comment: None,
            },
        );
        snapshot.save(&out_dir).expect("failed to save snapshot");

        let config = Config {
            root: temp_dir.path().to_path_buf(),
            out: out_dir,
            dialect: SchemaDialect::Postgres,
            ..Config::default()
        };

        let base_snapshot = load_base_snapshot(&config).expect("failed to load base snapshot");

        assert!(base_snapshot.tables.contains_key("users"));
    }
}
