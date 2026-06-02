use crate::Config;
use crate::Result;
use crate::diff::detect_nested_renames;
use crate::diff::diff_snapshots;
use crate::diff::rename::RenameDecision;
use crate::diff::rename::RenameScenario;
use crate::engines::Engine;
use crate::models::iden::Iden;
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::generator::{SqlGenerator, SqlOutput};
use crate::tui::prompt_for_rename;

use colored::Colorize;
use std::fmt::Write as _;

fn get_current_snapshot(config: &Config) -> Snapshot {
    let mut empty = Snapshot::new(config.dialect);
    // if let Some(user_table) = snapshot
    //     .tables
    //     .iter()
    //     .find(|(id, _)| *id == &Iden::new("product", Some("public".to_string())))
    // {
    //     let mut table = user_table.1.clone();
    //     table.name = "item".to_string();
    //     let mut created_col = table.columns.swap_remove("created_at").expect("created");
    //     created_col.name = "created".to_string();
    //     table.columns.insert("created".into(), created_col);
    //
    //     let mut updated_col = table.columns.swap_remove("updated_at").expect("updated");
    //     updated_col.name = "updated".to_string();
    //     table.columns.insert("updated".into(), updated_col);
    //
    //     empty
    //         .tables
    //         .insert(Iden::new("item", table.schema.clone()), table);
    // }

    empty
}

/// generate migration from current snapshot diff
pub async fn cmd_generate(config: &Config) -> Result<()> {
    if let Some(url) = config.database_url.as_ref() {
        println!("\n{} {}\n", "URL".bold(), url.bright_green());
    } else {
        println!("{}", "No database url found".bright_yellow());
    }

    println!("{}", "Generating migration".cyan());

    // FIXME: actual snapshot loading
    let prev = get_current_snapshot(config);
    let curr = get_current_snapshot(config);

    let mut diff = diff_snapshots(&prev, &curr)?;

    if diff.has_rename_scenarios() {
        let mut results = prompt_for_rename(&diff.rename_scenarios).await?;
        let mut nested: Vec<RenameScenario> = Vec::new();
        let prev_tables = prev.tables();
        let curr_tables = curr.tables();
        for res in results.iter() {
            if let RenameDecision::Rename(rename) = res {
                let from = prev_tables
                    .get(&rename.source.table)
                    .expect("table should exist");
                let to = curr_tables
                    .get(&rename.target.table)
                    .expect("table should exist");
                detect_nested_renames(from, to, &mut nested, false);
            }
        }
        if !nested.is_empty() {
            let nested_results = prompt_for_rename(&nested).await?;
            results.extend(nested_results);
        }

        diff.rename_scenarios.extend(nested);
        diff = diff.apply_rename_decisions(&results)?;
    }

    let generator = SqlGenerator::new(&config.dialect);
    let content = generator.generate_string(&diff.statements)?;
    println!("{}", content);

    Ok(())
}
