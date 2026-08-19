use std::fmt::Display;

use crate::diff::rename::RenameMap;
use crate::{Result, ShkiError};

use super::diff::rename::{RenameDecision, RenameKind, RenameScenario, RenameSelection};
use owo_colors::OwoColorize;
use inquire::Select;

pub async fn select_rename<Opt: Display>(
    prompt: impl Into<String>,
    options: Vec<Opt>,
) -> Result<Opt> {
    let ans = Select::new(&prompt.into(), options)
        .with_vim_mode(true)
        .without_filtering()
        .prompt()
        .map_err(ShkiError::Input)?;

    Ok(ans)
}

pub async fn prompt_for_rename(scenarios: &Vec<RenameScenario>) -> Result<Vec<RenameDecision>> {
    let mut decisions = Vec::new();
    for scenario in scenarios {
        let mut dropped = scenario.dropped.clone();
        for (_, id) in scenario.created.iter() {
            let mut options = vec![RenameSelection::Create(id.clone())];
            options.extend(dropped.iter().map(|(_, r)| RenameSelection::Rename {
                name: r.clone(),
                new_name: id.clone(),
            }));
            if options.len() == 1 {
                continue;
            }

            let prompt = if matches!(&scenario.kind, RenameKind::Table) {
                format!("{}", "Rename Table".bold())
            } else {
                scenario
                    .table
                    .clone()
                    .map(|t| format!("Table: {}", t.name.bold()))
                    .unwrap_or(format!("{}", "Rename Table".bold()))
            };
            let ans = select_rename(prompt, options).await?;

            if let RenameSelection::Rename { name, new_name } = ans {
                let decision = RenameDecision::Rename(RenameMap {
                    source: name.clone(),
                    target: new_name.clone(),
                });
                decisions.push(decision);
                dropped.swap_remove(&name.name);
            }
        }
    }
    Ok(decisions)
}
