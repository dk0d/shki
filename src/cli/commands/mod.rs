pub mod codegen;
mod create;
mod diff;
mod down;
mod generate;
mod init;
pub mod introspect;
mod migrate;
mod pull;
mod status;

use crate::config::Config;
use crate::{Commands, Result};

use super::{Cli, SchemaLanguage};

pub async fn run(cli: Cli) -> Result<()> {
    // Load config
    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        Config::default()
    };

    // Override with CLI args
    if let Some(dialect) = cli.dialect {
        config.dialect = dialect.into();
    }
    if let Some(url) = cli.database_url {
        config.database_url = Some(url);
    }
    if let Some(out) = cli.out {
        config.out = out;
    }

    config.verbose = cli.verbose;

    match cli.command {
        Commands::Init {
            path,
            dialect,
            language,
            simple,
        } => init::cmd_init(&path, dialect.map(Into::into), language, simple).await,
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
        Commands::Migrate { dry_run } => migrate::cmd_migrate(&config, dry_run).await,

        Commands::Generate {
            name,
            schema,
            dry_run,
        } => generate::cmd_generate_sql(&config, name, schema, dry_run),

        Commands::Status => status::cmd_status(&config).await,

        Commands::Pull {
            format,
            output,
            with_migration_table,
        } => pull::cmd_pull(&config, &format, output.as_deref(), with_migration_table).await,

        Commands::Diff { schema, sql } => diff::cmd_diff(&config, schema.as_deref(), sql).await,

        Commands::Codegen {
            out,
            mode,
            schema,
            verbose,
        } => codegen::cmd_codegen(&config, schema, mode, out, Some(verbose)),

        Commands::Down { count, dry_run } => down::cmd_down(&config, count, dry_run).await,
    }
}
