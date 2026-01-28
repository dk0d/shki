//! CLI implementation for Shki
//!
//! This module provides the command-line interface for the shki tool.

pub mod commands;
use clap::builder::styling::{AnsiColor, Color, Style};
pub use commands::run;
pub mod constants;
pub mod templates;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::schema::SchemaDialect;

pub fn get_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .usage(
            Style::new()
                .bold()
                .underline()
                .fg_color(Some(AnsiColor::Yellow.into())),
        )
        .header(
            Style::new()
                .bold()
                .underline()
                .fg_color(Some(AnsiColor::Blue.into())),
        )
        .literal(Style::new().fg_color(Some(AnsiColor::Green.into())))
        .invalid(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
        .error(Style::new().bold().fg_color(Some(AnsiColor::Red.into())))
        .valid(
            Style::new()
                .bold()
                .underline()
                .fg_color(Some(AnsiColor::Green.into())),
        )
        .placeholder(Style::new().fg_color(Some(AnsiColor::White.into())))
}

/// Shki - Declarative database schema management
#[derive(Parser, Debug)]
#[command(name = "shki")]
#[command(author, version, about, long_about = None, styles=get_styles())]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true, default_value = "shki.toml")]
    pub config: PathBuf,

    /// Database dialect
    #[arg(long, short = 'l', global = true, value_enum)]
    pub dialect: Option<DialectArg>,

    /// Database connection URL
    #[arg(long, short = 'u', global = true, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    /// Output directory for migrations
    #[arg(short, long, global = true)]
    pub out: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Dialect argument for CLI
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DialectArg {
    Postgres,
    Postgresql,
    Mysql,
    Sqlite,
}

impl From<DialectArg> for SchemaDialect {
    fn from(arg: DialectArg) -> Self {
        match arg {
            DialectArg::Postgres | DialectArg::Postgresql => SchemaDialect::Postgres,
            DialectArg::Mysql => SchemaDialect::Mysql,
            DialectArg::Sqlite => SchemaDialect::Sqlite,
        }
    }
}

/// Schema definition language
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum SchemaLanguage {
    /// Define schemas using Rust code
    // Rust,

    /// Define schemas using Lua scripts
    #[default]
    Lua,
}

/// CLI commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new shki project
    Init {
        /// Target directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Database dialect
        #[arg(long, value_enum)]
        dialect: Option<DialectArg>,

        /// Schema definition language (lua or rust)
        #[arg(short, long, value_enum, default_value = "lua")]
        language: SchemaLanguage,
    },

    /// Generate a migration from schema changes
    Generate {
        /// Migration name/suffix
        #[arg(short, long)]
        name: Option<String>,

        /// Path to schema file(s) or directory
        #[arg(short, long)]
        schema: Option<PathBuf>,

        /// Don't create migration files, just print SQL
        #[arg(long)]
        dry_run: bool,
    },

    /// Apply pending migrations to the database
    Migrate {
        /// Only show what would be applied
        #[arg(long, short)]
        dry_run: bool,
    },

    /// Push schema changes directly to the database (no migration files)
    Push {
        /// Path to schema file(s) or directory
        #[arg(short, long)]
        schema: Option<PathBuf>,

        /// Skip confirmation for destructive changes
        #[arg(long)]
        force: bool,

        /// Only show what would be executed
        #[arg(long)]
        dry_run: bool,
    },

    /// Pull (introspect) the database schema
    Pull {
        /// Output format (json, sql, rust)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Output file (defaults to stdout)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Show the diff between schema and database
    Diff {
        /// Path to schema file(s) or directory
        #[arg(short, long)]
        schema: Option<PathBuf>,

        /// Output as SQL instead of summary
        #[arg(long)]
        sql: bool,
    },

    /// List migrations and their status
    Status,

    /// Check migration files for consistency
    Check,

    /// Drop a migration file
    Drop {
        /// Migration name or index to drop
        migration: String,
    },

    /// Export schema as SQL
    Export {
        /// Path to schema file(s) or directory
        #[arg(short, long)]
        schema: Option<PathBuf>,

        /// Output file (defaults to stdout)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Create a blank migration file for manual SQL editing
    Create {
        /// Migration name (e.g., "add_user_index", "create_audit_table")
        name: String,

        /// Initial SQL content to include in the migration
        #[arg(long)]
        sql: Option<String>,

        /// Read initial SQL from a file
        #[arg(long, conflicts_with = "sql")]
        sql_file: Option<PathBuf>,

        /// Also create a down migration file (.down.sql)
        #[arg(long)]
        with_down: bool,

        /// Open the created file in the default editor
        #[arg(short, long)]
        edit: bool,
    },

    /// Rollback (undo) applied migrations using down migration files
    Down {
        /// Number of migrations to rollback (default: all available)
        #[arg(short = 'n', long)]
        count: Option<usize>,

        /// Only show what would be rolled back
        #[arg(long)]
        dry_run: bool,
    },
}
