#![allow(dead_code)]
pub mod cli;
pub use cli::*;
pub mod config;
pub mod domain;
pub use domain::*;
pub mod errors;
pub use errors::*;
pub mod engines;

use crate::config::Config;

pub use crate::Result;

use self::domain::display::tables::display_config;

pub const MIGRATION_SPLIT_MARKER: &str = "--> +statement";

pub async fn run(cli: Cli) -> Result<()> {
    // Load config
    let mut config = Config::load(&cli.config, &cli.common)?;

    // // Override with CLI args
    // if let Some(url) = cli.database_url {
    //     config.database_url = Some(url);
    // }
    // if let Some(out) = cli.out {
    //     config.out = out;
    // }

    config.verbose = cli.common.verbose;

    match cli.command {
        Commands::Config => {
            display_config(&config);
            Ok(())
        }
        Commands::Init {
            path,
            dialect,
            // language,
            mode,
        } => init::cmd_init(&path, dialect, mode).await,
        Commands::Create {
            name,
            sql,
            sql_file,
            with_down,
            edit,
        } => {
            create::cmd_create(
                &config,
                &name,
                sql.as_deref(),
                sql_file.as_deref(),
                with_down,
                edit,
            )
            .await
        }
        Commands::Status => status::cmd_status(&config).await,
        Commands::Migrate => migrate::cmd_migrate(&config).await, //
        //     Commands::Generate {
        //         name,
        //         schema,
        //         dry_run,
        //     } => generate::cmd_generate_sql(&config, name, schema, dry_run),
        //
        //     Commands::Status => status::cmd_status(&config).await,
        //
        //     Commands::Pull {
        //         format,
        //         output,
        //         with_migration_table,
        //     } => pull::cmd_pull(&config, &format, output.as_deref(), with_migration_table).await,
        //
        //     Commands::Bootstrap {
        //         name,
        //         legacy_tables,
        //         drop_legacy_tables,
        //         write_lua,
        //         lua_output,
        //         no_mark_applied,
        //         dry_run,
        //         force,
        //     } => {
        //         bootstrap::cmd_bootstrap(
        //             &config,
        //             name,
        //             legacy_tables,
        //             drop_legacy_tables,
        //             write_lua,
        //             lua_output,
        //             !no_mark_applied,
        //             dry_run,
        //             force,
        //         )
        //         .await
        //     }
        //
        //     Commands::Squash {
        //         name,
        //         dry_run,
        //         force,
        //     } => squash::cmd_squash(&config, name, dry_run, force).await,
        //
        //     Commands::Diff { schema, sql } => diff::cmd_diff(&config, schema.as_deref(), sql).await,
        //
        //     Commands::Drop { migration } => drop::cmd_drop(&config, &migration).await,
        //
        //     Commands::Codegen {
        //         out,
        //         mode,
        //         schema,
        //         language,
        //         verbose,
        //     } => codegen::cmd_codegen(&config, schema, mode, out, Some(verbose), language),
        //
        Commands::Down { count, dry_run } => down::cmd_down(&config, count, dry_run).await,
    }
}
