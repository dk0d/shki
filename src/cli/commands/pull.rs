use super::introspect::introspect_db;
use crate::Config;
use crate::{Result, ShkiError, Snapshot, diff::diff_snapshots, sql::SqlGenerator};

use colored::Colorize;
use std::fmt::Write as _;

/// Pull (introspect) the database schema
pub async fn cmd_pull(
    config: &Config,
    format: &str,
    output: Option<&std::path::Path>,
    with_migration_table: bool,
) -> Result<()> {
    println!("{}", "Introspecting database...".cyan());
    let snapshot = if with_migration_table {
        introspect_db(config).await?
    } else {
        let mut snapshot = introspect_db(config).await?;
        snapshot.tables.shift_remove(&config.migrations.table);
        snapshot
    };

    let content = match format {
        "json" => snapshot.to_json()?,
        "sql" => {
            // Generate CREATE statements
            let empty = Snapshot::new(config.dialect);
            let diff = diff_snapshots(&empty, &snapshot)?;
            let generator = SqlGenerator::new(config.dialect);
            generator.generate_sql(&diff)?
        }
        "rust" => {
            // Generate Rust schema code
            generate_rust_schema(&snapshot)?
        }
        _ => {
            return Err(ShkiError::config(format!("Unknown format: {}", format)));
        }
    };

    match output {
        Some(path) => {
            // Resolve the output path relative to the project root
            let resolved_path = config.resolve_path(path);
            std::fs::write(&resolved_path, &content)?;
            println!(
                "{} {}",
                "Schema written to:".green(),
                resolved_path.display()
            );
        }
        None => {
            println!("{}", content);
        }
    }

    Ok(())
}

/// Generate Rust schema code from a snapshot
fn generate_rust_schema(snapshot: &Snapshot) -> Result<String> {
    let mut code = String::new();

    code.push_str("//! Auto-generated schema from database introspection\n\n");
    code.push_str("use shki::prelude::*;\n\n");

    // Generate enums
    for enum_snapshot in snapshot.enums.values() {
        writeln!(&mut code, "// Enum: {}", enum_snapshot.name)
            .expect("writing to String cannot fail");
        writeln!(&mut code, "// Values: {:?}", enum_snapshot.values)
            .expect("writing to String cannot fail");
        writeln!(&mut code).expect("writing to String cannot fail");
    }

    // Generate table definitions
    for table in snapshot.tables.values() {
        writeln!(&mut code, "// Table: {}", table.name).expect("writing to String cannot fail");
        writeln!(&mut code, "pub fn {}() -> Table {{", table.name)
            .expect("writing to String cannot fail");
        writeln!(&mut code, "    TableBuilder::new(\"{}\")", table.name)
            .expect("writing to String cannot fail");

        for col in table.columns.values() {
            writeln!(
                &mut code,
                "        // {}: {}{}",
                col.name,
                col.data_type,
                if col.nullable { "" } else { " NOT NULL" }
            )
            .expect("writing to String cannot fail");
        }

        code.push_str("        .build()\n");
        code.push_str("}\n\n");
    }

    Ok(code)
}
