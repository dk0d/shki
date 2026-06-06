//! CLI implementation for Shki
//!
//! This module provides the command-line interface for the shki tool.

// pub mod commands;
use clap::builder::styling::{AnsiColor, Color, Style};
// pub use commands::run;
// pub mod constants;
// pub mod templates;

use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// use crate::model::schema::SchemaDialect;

// use self::commands::codegen::OutputMode;
// use self::commands::codegen::languages::TypescriptFlavor;
pub use CodegenLanguage as LanguageArg;

use crate::codegen::OutputMode;
use crate::config::{MigrationPrefix, SchemaMode};
use crate::domain::codegen::lang::typescript::TypescriptFlavor;
use crate::dump::SchemaExportFormat;
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

    /// Database connection URL
    #[arg(long, short = 'u', global = true, env = "DATABASE_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    /// Shadow Database connection URL used to compile Declarative Schemas
    #[arg(long, global = true, env = "SHKI_SHADOW_DATABASE_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_database_url: Option<String>,

    /// PostgreSQL major version for embedded Shadow Database compilation
    #[arg(long, global = true, env = "SHKI_PG_VERSION")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pg_version: Option<u8>,

    /// Directory to output and read migrations
    #[arg(short, long, global = true)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrations_dir: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    #[serde(skip_serializing_if = "crate::config::is_false")]
    pub verbose: bool,

    #[command(flatten)]
    #[serde(default, skip_serializing_if = "MigrationArgs::is_empty")]
    pub migrations: MigrationArgs,

    #[arg(short, long, global = true, default_value_t = false)]
    #[serde(default, skip_serializing_if = "crate::config::is_false")]
    pub no_color: bool,
}

#[derive(Debug, Serialize, Args, Default)]
pub struct MigrationArgs {
    /// Name of the migrations table
    #[arg(short='T',long,  default_value = None)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,

    /// Schema for the migrations table (PostgreSQL)
    #[arg(short='S', long,  default_value = None)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

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
        self.table.is_none()
            && self.schema.is_none()
            && self.prefix.is_none()
            && !self.generate_down
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

        /// Schema definition language (sql or lua)
        #[arg(short, long, value_enum)]
        mode: Option<SchemaMode>,
    },
    /// Generate a schema-derived migration from the current Declarative Schema
    #[command(visible_alias = "gen")]
    Generate {
        /// Migration name/suffix
        name: String,

        /// Create a Custom Migration instead of a schema-derived migration
        #[arg(long)]
        custom: bool,

        /// Also generate a Down Migration
        #[arg(short = 'd', long)]
        with_down: bool,
    },

    /// Apply pending migrations to the database
    #[command(visible_alias = "up")]
    Migrate {
        /// Only show what would be applied
        #[arg(long, short)]
        dry_run: bool,
    },

    /// Create a blank migration file for manual SQL editing
    #[command(visible_alias = "new")]
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

    // /// Bootstrap shki from an existing database state
    // #[command(visible_alias = "strap")]
    // Bootstrap {
    //     /// Migration name/suffix for the generated initial migration
    //     name: Option<String>,
    //
    //     /// Legacy migration table(s) to exclude and optionally drop
    //     #[arg(long = "legacy-table")]
    //     legacy_tables: Vec<String>,
    //
    //     /// Drop provided legacy migration table(s) from the database
    //     #[arg(long, default_value_t = false)]
    //     drop_legacy_tables: bool,
    //
    //     /// Also generate Lua schema definitions from the introspected snapshot
    //     #[arg(long, default_value_t = false)]
    //     write_lua: bool,
    //
    //     /// Path to write generated Lua schema (defaults to config schema path)
    //     #[arg(long)]
    //     lua_output: Option<PathBuf>,
    //
    //     /// Do not record the generated bootstrap migration as already applied
    //     #[arg(long, action = ArgAction::SetTrue, default_value_t = false)]
    //     no_mark_applied: bool,
    //
    //     /// Show what would be generated without writing files or changing DB
    //     #[arg(long, short)]
    //     dry_run: bool,
    //
    //     /// Allow running even when migrations/snapshots already exist locally
    //     #[arg(long, default_value_t = false)]
    //     force: bool,
    // },
    //
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
    Diff,

    /// List migrations and their status
    #[command(visible_alias = "s")]
    Status,

    /// Generate Rust structs/enums from database schema
    #[command(visible_alias = "code")]
    Codegen {
        /// Output language (rust, protobuf, ts)
        #[command(subcommand)]
        language: CodegenLanguage,

        /// Path to output directory
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Path to schema file(s) or directory
        #[arg(short, long)]
        schema: Option<PathBuf>,

        /// Output mode (module or single file)
        #[arg(long, short)]
        mode: Option<OutputMode>,

        /// Enable verbose output
        /// Will print the generated code to stdout as well as writing to files
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },

    // /// Drop a migration file
    // #[command()]
    // Drop {
    //     /// Migration name or index to drop
    //     #[arg(default_value=None)]
    //     migration: Option<String>,
    // },
    //
    /// Rollback (undo) applied migrations using down migration files
    Down {
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
