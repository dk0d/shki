mod init;
mod migrate;

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
        } => init::cmd_init(&path, dialect.map(Into::into), language).await,
        Commands::Migrate { dry_run } => migrate::cmd_migrate(&config, dry_run).await,
        _ => todo!(),
    }
}
