use crate::config::Config;

use crate::migrate::manager::MigrationManager;
use crate::utils::resolve_path;
use crate::{Result, ShkiError};
use owo_colors::OwoColorize;

/// Create a blank migration file for manual editing
pub async fn cmd_create(
    config: &Config,
    name: &str,
    sql: Option<&str>,
    sql_file: Option<&std::path::Path>,
    with_down: bool,
    open_editor: bool,
) -> Result<()> {
    let migration_manager = MigrationManager::from_config(config).await?;

    // Use config setting for down migrations, but CLI flag can override
    let create_down = with_down || config.migrations.generate_down();

    // Get initial SQL content if provided
    let initial_sql = if let Some(sql_content) = sql {
        Some(sql_content.to_string())
    } else if let Some(file_path) = sql_file {
        // Resolve the SQL file path relative to the project root
        let resolved_path = resolve_path(None, file_path);
        Some(
            std::fs::read_to_string(&resolved_path)
                .map_err(|e| ShkiError::config(format!("Failed to read SQL file: {}", e)))?,
        )
    } else {
        None
    };

    // Create the migration file(s)
    let (up_path, down_path) = if let Some(sql_content) = initial_sql.as_deref() {
        if create_down {
            // Create with down migration (empty down template)
            let (up, down) = migration_manager.create_blank_migration_with_content_and_down(
                name,
                Some(sql_content),
                Some("-- Add rollback SQL here\n"),
            )?;
            (up, down)
        } else {
            (
                migration_manager.create_blank_migration_with_content(name, Some(sql_content))?,
                None,
            )
        }
    } else {
        migration_manager.create_blank_migration_with_down(name, create_down)?
    };

    let migration_name = up_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    println!("{} {}", "Created migration:".green(), migration_name);
    println!("  Up:   {}", up_path.display());
    if let Some(ref down) = down_path {
        println!("  Down: {}", down.display());
    }

    // Open in editor if requested
    if open_editor {
        open_in_editor(&up_path)?;
    } else {
        println!(
            "\n{}: Edit the file(s) and add your SQL statements",
            "Next steps".cyan()
        );
        println!(
            "  Then run '{}' to apply the migration",
            "shki migrate".yellow()
        );
        if down_path.is_some() {
            println!("  Use '{}' to rollback migrations", "shki down".yellow());
        }
    }

    Ok(())
}

/// Open a file in the system's default editor
fn open_in_editor(path: &std::path::Path) -> Result<()> {
    // Try common editor environment variables
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            // Fallback to common editors
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else if cfg!(target_os = "macos") {
                "open -t".to_string()
            } else {
                "vi".to_string()
            }
        });

    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| ShkiError::config("Invalid editor command"))?;

    let status = std::process::Command::new(cmd)
        .args(args.iter())
        .arg(path)
        .status()
        .map_err(|e| ShkiError::config(format!("Failed to open editor: {}", e)))?;

    if !status.success() {
        return Err(ShkiError::config(format!(
            "Editor exited with status: {}",
            status
        )));
    }

    Ok(())
}
