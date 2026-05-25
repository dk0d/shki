use crate::Config;
use crate::PullFormat;
use crate::Result;
use crate::diff::diff_snapshots;
use crate::engines::Engine;
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::generator::SqlGenerator;
use crate::utils::resolve_path;

use colored::Colorize;

/// Pull (introspect) the database schema
pub async fn cmd_pull(
    config: &Config,
    format: &PullFormat,
    output: &Option<&std::path::Path>,
    schema: &Option<String>,
) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

    println!("{}", "Introspecting database...\n".cyan());

    let engine = Engine::from_config(config).await?;
    let snapshot = engine.introspect(config, schema).await?;

    let content = match format {
        PullFormat::Json => snapshot.to_json()?,
        PullFormat::Sql => {
            // Generate CREATE statements
            let empty = Snapshot::new(config.dialect);
            let diff = diff_snapshots(&empty, &snapshot)?;
            let generator = SqlGenerator::new(&config.dialect);
            // generator.generate_string(&diff.statements)?;
            generator.generate_string(&diff.statements)?
        } // "rust" => {
          // Generate Rust schema code
          // generate_rust_schema(&snapshot)?
          // }
    };

    match output {
        Some(path) => {
            // Resolve the output path relative to the project root
            let resolved_path = resolve_path(None, path);
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
    //
    Ok(())
}

// Generate Rust schema code from a snapshot

// fn generate_rust_schema(snapshot: &Snapshot) -> Result<String> {
//     let mut code = String::new();
//
//     code.push_str("//! Auto-generated schema from database introspection\n\n");
//     code.push_str("use shki::prelude::*;\n\n");
//
//     // Generate enums
//     for enum_snapshot in snapshot.enums.values() {
//         writeln!(&mut code, "// Enum: {}", enum_snapshot.name)
//             .expect("writing to String cannot fail");
//         writeln!(&mut code, "// Values: {:?}", enum_snapshot.values)
//             .expect("writing to String cannot fail");
//         writeln!(&mut code).expect("writing to String cannot fail");
//     }
//
//     // Generate table definitions
//     for table in snapshot.tables.values() {
//         writeln!(&mut code, "// Table: {}", table.name).expect("writing to String cannot fail");
//         writeln!(&mut code, "pub fn {}() -> Table {{", table.name)
//             .expect("writing to String cannot fail");
//         writeln!(&mut code, "    TableBuilder::new(\"{}\")", table.name)
//             .expect("writing to String cannot fail");
//
//         for col in table.columns.values() {
//             writeln!(
//                 &mut code,
//                 "        // {}: {}{}",
//                 col.name,
//                 col.data_type,
//                 if col.nullable { "" } else { " NOT NULL" }
//             )
//             .expect("writing to String cannot fail");
//         }
//
//         code.push_str("        .build()\n");
//         code.push_str("}\n\n");
//     }
//
//     Ok(code)
// }
