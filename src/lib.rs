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
    let config = Config::load(&cli.config, &cli.common)?;

    match cli.command {
        Commands::Config => {
            display_config(&config);
            Ok(())
        }

        Commands::Init { path, dialect } => init::cmd_init(&path, dialect).await,

        Commands::Create {
            migrations,
            name,
            sql,
            sql_file,
            with_down,
            edit,
        } => {
            let config = config.with_migration_args(&migrations)?;
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

        Commands::Generate {
            shadow,
            migrations,
            name,
            custom,
            with_down,
        } => {
            let config = config
                .with_shadow_args(&shadow)?
                .with_migration_args(&migrations)?;
            generate::cmd_generate(&config, &name, custom, with_down).await
        }

        Commands::Status { migrations } => {
            let config = config.with_migration_args(&migrations)?;
            status::cmd_status(&config).await
        }

        Commands::Migrate {
            migrations,
            mode,
            dry_run,
        } => {
            let config = config.with_migration_args(&migrations)?;
            migrate::cmd_migrate(&config, mode, dry_run).await
        }

        Commands::Dump {
            format,
            output,
            dirs,
            force,
            schema,
        } => dump::cmd_dump(&config, &format, output.as_deref(), dirs, force, &schema).await,

        Commands::Bootstrap {
            migrations,
            name,
            dry_run,
            force,
            schema,
        } => {
            let config = config.with_migration_args(&migrations)?;
            bootstrap::cmd_bootstrap(&config, name.as_deref(), dry_run, force, &schema).await
        }

        Commands::Adopt {
            migrations,
            name,
            mark_only,
            force,
            dry_run,
            schema,
        } => {
            let config = config.with_migration_args(&migrations)?;
            adopt::cmd_adopt(&config, name.as_deref(), mark_only, force, dry_run, &schema).await
        }

        Commands::Diff { shadow } => {
            let config = config.with_shadow_args(&shadow)?;
            diff::cmd_diff(&config).await
        }

        Commands::Drop { migration } => drop_migration::cmd_drop(&config, &migration).await,
        Commands::Queries { shadow, querygen } => {
            let preview = querygen.preview;
            let config = config
                .with_shadow_args(&shadow)?
                .with_querygen_args(&querygen)?;
            codegen::queries::cmd_query_codegen(&config, preview).await
        }
        Commands::Codegen {
            shadow,
            codegen: codegen_args,
            source: schema,
            language,
        } => {
            let preview = codegen_args.preview;
            let config = config
                .with_shadow_args(&shadow)?
                .with_codegen_args(&codegen_args)?;
            codegen::cmd_codegen(&config, schema, language, preview).await
        }

        Commands::Down {
            migrations,
            count,
            dry_run,
        } => {
            let config = config.with_migration_args(&migrations)?;
            down::cmd_down(&config, count, dry_run).await
        }
    }
}
