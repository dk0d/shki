//! CLI implementation for Shki
//!
//! This module provides the command-line interface for the shki tool.

// pub mod commands;
use clap::builder::styling::{AnsiColor, Color, Style};
// pub use commands::run;
// pub mod constants;
// pub mod templates;

use clap::{ArgAction, Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// use crate::model::schema::SchemaDialect;

// use self::commands::codegen::OutputMode;
// use self::commands::codegen::languages::TypescriptFlavor;
pub use CodegenLanguage as LanguageArg;

use crate::codegen::OutputMode;
use crate::config::MigrationPrefix;
use crate::domain::codegen::lang::typescript::TypescriptFlavor;
use crate::dump::SchemaExportFormat;
use crate::migrate::manager::ApplyMigrationMode;
use crate::schema::SqlDialect;

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

#[derive(Debug, Serialize, Parser, Default)]
pub struct CommonArgs {
    /// Database dialect
    #[arg(long, short = 'l', global = true, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<SqlDialect>,

    /// Schema for the migrations table (PostgreSQL)
    #[arg(short='S', long,  default_value = None)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Database connection URL
    #[arg(long, short = 'u', global = true, env = "DATABASE_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    /// Directory to output and read migrations
    #[arg(short = 'd', long = "dir", long, global = true)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrations_dir: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    #[serde(skip_serializing_if = "crate::config::is_false")]
    pub verbose: bool,

    #[arg(short, long, global = true, default_value_t = false)]
    #[serde(default, skip_serializing_if = "crate::config::is_false")]
    pub no_color: bool,
}

#[derive(Debug, Clone, Serialize, Args, Default)]
pub struct ShadowArgs {
    /// Shadow Database connection URL used to compile Declarative Schemas
    #[arg(long, env = "SHKI_SHADOW_DATABASE_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_database_url: Option<String>,

    /// PostgreSQL major version for embedded Shadow Database compilation
    #[arg(long, env = "SHKI_PG_VERSION")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pg_version: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Args, Default)]
pub struct CodegenArgs {
    /// Output directory for generated code
    #[arg(short, long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,

    /// Output mode: single file or module directory
    #[arg(long, short)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputMode>,

    /// Whether to add serde derives and rename attributes
    #[arg(long, action = ArgAction::SetTrue)]
    #[serde(default, skip_serializing_if = "crate::config::is_false")]
    pub serde: bool,

    /// Whether to generate sqlx::FromRow derive
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_sqlx")]
    pub sqlx: bool,

    /// Disable sqlx derives
    #[arg(long = "no-sqlx", action = ArgAction::SetTrue)]
    pub no_sqlx: bool,

    /// Preview the output without writing anything
    #[arg(long, action = ArgAction::SetFalse)]
    pub preview: bool,
}

impl CodegenArgs {
    pub fn is_empty(&self) -> bool {
        self.output.is_none() && self.format.is_none() && !self.serde && !self.sqlx && !self.no_sqlx
    }
}

#[derive(Debug, Clone, Serialize, Args, Default)]
pub struct MigrationArgs {
    /// Name of the migrations table
    #[arg(short='T',long,  default_value = None)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,

    /// Migration file name prefix style
    #[arg(
        long,
        value_enum,
        default_value = None
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<MigrationPrefix>,

    /// Whether to generate down migrations alongside up migrations
    #[arg(long, default_value_t = false)]
    #[serde(skip_serializing_if = "crate::config::is_false")]
    pub generate_down: bool,
}

impl MigrationArgs {
    pub fn is_empty(&self) -> bool {
        self.table.is_none() && self.prefix.is_none() && !self.generate_down
    }
}

/// Shki - Declarative database schema management
#[derive(Parser, Debug, Serialize)]
#[command(name = "shki")]
#[command(author, version, about, long_about = None, styles=get_styles())]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true, default_value = "shki.toml")]
    pub config: PathBuf,

    #[command(flatten)]
    pub common: CommonArgs,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Schema definition language
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum, Serialize, Deserialize)]
pub enum ShkiMode {
    /// Define schemas using Rust code
    // Rust,

    /// Define schemas using Typescript
    #[default]
    #[serde(alias = "ts")]
    #[value(alias = "ts")]
    Typscript,
}

/// Output language for code generation
/// Commands for specifying which language to generate code for
/// also allows for language specific flags and configuration
#[derive(Subcommand, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CodegenLanguage {
    #[command(visible_alias = "rs")]
    Rust,

    #[command(visible_alias = "ts")]
    Typescript { flavor: TypescriptFlavor },

    #[command(visible_alias = "proto")]
    Protobuf,
}

/// CLI commands
#[derive(Subcommand, Debug, Serialize)]
pub enum Commands {
    /// Print the current configuration
    #[command(visible_alias = "conf")]
    Config,

    /// Initialize a new shki project
    ///
    #[command(visible_alias = "i")]
    Init {
        /// Target directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Database dialect
        #[arg(long, value_enum)]
        dialect: Option<SqlDialect>,
    },
    /// Generate a schema-derived migration from the current Declarative Schema
    #[command(visible_alias = "gen")]
    Generate {
        #[command(flatten)]
        shadow: ShadowArgs,

        #[command(flatten)]
        migrations: MigrationArgs,

        /// Migration name/suffix
        name: String,

        /// Create a Custom Migration instead of a schema-derived migration
        #[arg(long)]
        custom: bool,

        /// Also generate a Down Migration
        #[arg(long, long = "down")]
        with_down: bool,
    },

    /// Apply pending migrations to the database
    #[command(visible_alias = "up")]
    Migrate {
        #[command(subcommand)]
        mode: Option<ApplyMigrationMode>,

        #[command(flatten)]
        migrations: MigrationArgs,

        /// Only show what would be applied
        #[arg(long, long = "dry")]
        dry_run: bool,
    },

    /// Create a blank migration file for manual SQL editing
    #[command(visible_alias = "new")]
    Create {
        #[command(flatten)]
        migrations: MigrationArgs,

        /// Migration name (e.g., "add_user_index", "create_audit_table")
        name: String,

        /// Initial SQL content to include in the migration
        #[arg(long)]
        sql: Option<String>,

        /// Read initial SQL from a file
        #[arg(long, conflicts_with = "sql")]
        sql_file: Option<PathBuf>,

        /// Also create a down migration file (.down.sql)
        #[arg(short = 'd', long)]
        with_down: bool,

        /// Open the created file in the default editor
        #[arg(short, long)]
        edit: bool,
    },

    /// Dump the live database shape as a Declarative Schema
    Dump {
        /// Output format
        #[arg(short, long, default_value_t = SchemaExportFormat::default(), value_enum)]
        format: SchemaExportFormat,

        /// Output file (defaults to stdout)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Emit a Directory Schema with main.sql as the canonical entrypoint
        #[arg(long, alias = "multi-file")]
        dirs: bool,

        /// Overwrite generated file collisions in directory mode
        #[arg(long)]
        force: bool,

        /// Schema to dump (Postgres, defaults to public)
        #[arg(long)]
        schema: Option<String>,
    },

    /// Author an initial baseline migration from an existing database
    #[command(visible_alias = "strap")]
    Bootstrap {
        #[command(flatten)]
        migrations: MigrationArgs,

        /// Migration name/suffix for the generated initial migration (defaults to 'bootstrap')
        name: Option<String>,

        /// Show what would be generated without writing files or changing DB
        #[arg(long, short)]
        dry_run: bool,

        /// Allow running even when migrations/snapshots already exist locally
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Schema to bootstrap (Postgres, defaults to public)
        #[arg(long)]
        schema: Option<String>,
    },

    /// Adopt an existing database at a committed baseline, then apply newer migrations
    #[command(visible_alias = "baseline")]
    Adopt {
        #[command(flatten)]
        migrations: MigrationArgs,

        /// Migration to adopt up to (defaults to the earliest schema migration)
        name: Option<String>,

        /// Mark the baseline applied but do not apply newer pending migrations
        #[arg(long, action = ArgAction::SetTrue, default_value_t = false)]
        mark_only: bool,

        /// Mark applied even if the live database differs from the baseline snapshot
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Show what would be validated, marked, and applied without changing anything
        #[arg(long, short)]
        dry_run: bool,

        /// Schema to introspect for validation (Postgres, defaults to public)
        #[arg(long)]
        schema: Option<String>,
    },

    // /// Squash existing shki migrations into a single baseline migration
    // #[command(visible_alias = "sq")]
    // Squash {
    //     /// Migration name/suffix for the squashed migration
    //     #[arg(long)]
    //     name: Option<String>,
    //
    //     /// Show what would happen without writing files or changing DB
    //     #[arg(long, short)]
    //     dry_run: bool,
    //
    //     /// Allow squash even if local outputs already look unusual
    //     #[arg(long, default_value_t = false)]
    //     force: bool,
    // },
    //
    /// Preview changes between latest Snapshot and current Declarative Schema
    Diff {
        #[command(flatten)]
        shadow: ShadowArgs,
    },

    /// List migrations and their status
    #[command(visible_alias = "s")]
    Status {
        #[command(flatten)]
        migrations: MigrationArgs,
    },

    /// Generate types from database schema (Rust/Typescript/Protobuf)
    #[command(visible_alias = "code")]
    Codegen {
        #[command(flatten)]
        shadow: ShadowArgs,

        #[command(flatten)]
        codegen: CodegenArgs,

        /// Output language (rust, protobuf, ts)
        #[command(subcommand)]
        language: CodegenLanguage,

        /// Override path to schema file(s), schema directory, or snapshot json to use for generation
        #[arg(short, long)]
        source: Option<PathBuf>,
    },

    /// Drop a migration & snapshot (destructive)
    #[command()]
    Drop {
        /// Migration name or index to drop
        #[arg(default_value=None)]
        migration: Option<String>,
    },

    /// Rollback (undo) applied migrations using down migration files
    Down {
        #[command(flatten)]
        migrations: MigrationArgs,

        /// Number of migrations to rollback (default: 1)
        count: Option<usize>,

        /// Only show what would be rolled back
        #[arg(long)]
        dry_run: bool,
    },
    // Check migration files for consistency
    // Check,
    // Export schema as SQL
    // Export {
    //     /// Path to schema file(s) or directory
    //     #[arg(short, long)]
    //     schema: Option<PathBuf>,
    //
    //     /// Output file (defaults to stdout)
    //     #[arg(long)]
    //     output: Option<PathBuf>,
    // },
}
