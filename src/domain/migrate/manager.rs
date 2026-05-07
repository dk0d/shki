//! Migration file mb.rsanagement
//!
//! This module handles reading, writing, and applying migration files.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use petname::Generator;
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;
use sqlx::prelude::FromRow;

use super::checksum::sql_checksum;
use super::queries;
use super::utils::{generate_blank_migration_template, sanitize_migration_name, truncate_sql};
use crate::config::MigrationPrefix;
use crate::models::table_id::TableId;
use crate::schema::SqlDialect;
// use crate::error::{MismatchDetail, SnapshotValidationSummary};
// use crate::snapshot::Snapshot;
use crate::{MIGRATION_SPLIT_MARKER, Result, ShkiError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum MigrationDirection {
    Up,
    Down,
}

impl std::fmt::Display for MigrationDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationDirection::Up => write!(f, "up"),
            MigrationDirection::Down => write!(f, "down"),
        }
    }
}
/// A migration file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// Migration version/name
    pub name: String,

    /// SQL statements for the up migration
    pub sql: String,

    /// Timestamp when the migration was created
    pub created_at: DateTime<Utc>,

    /// Snapshot ID this migration was generated from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_snapshot: Option<String>,

    /// Snapshot ID this migration produces
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_snapshot: Option<String>,
}

pub fn option_truncate<T>(value: &Option<T>, default: &str, limit: usize) -> String
where
    T: ToString,
{
    match value {
        Some(val) => {
            let text = val.to_string();
            if text.len() > limit {
                let text = tabled::settings::width::Truncate::truncate(&text, limit);
                return format!("{}...", &text);
            }
            text
        }
        None => default.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, tabled::Tabled)]
pub struct MigrationRow {
    pub id: i64,
    pub name: String,

    #[tabled(display("option_truncate", "", 5))]
    pub checksum: Option<String>,
    pub applied_at: String,
}

/// Migration manager
pub struct MigrationManager {
    /// Output directory
    pub out_dir: PathBuf,

    /// Migration table name
    pub table_name: String,

    /// Migration table schema (PostgreSQL)
    pub table_schema: Option<String>,

    /// Migration prefix style
    pub prefix: MigrationPrefix,

    /// Database dialect
    pub dialect: SqlDialect,
}

impl Default for MigrationManager {
    fn default() -> Self {
        Self {
            out_dir: "migrations".into(),
            table_name: "__shki_migrations".to_string(),
            table_schema: None,
            prefix: MigrationPrefix::Index,
            dialect: SqlDialect::Postgres,
        }
    }
}

impl MigrationManager {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            out_dir: config.out_dir(),
            table_name: config.migrations.table.clone(),
            table_schema: config.migrations.schema.clone(),
            prefix: config.migrations.prefix,
            dialect: config.dialect,
        }
    }

    /// Create a new migration manager
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            out_dir: out_dir.into(),
            ..Default::default()
        }
    }

    pub fn with_config(mut self, config: &crate::config::Config) -> Self {
        self.out_dir = config.out_dir();
        self.table_name = config.migrations.table.clone();
        self.table_schema = config.migrations.schema.clone();
        self.prefix = config.migrations.prefix;
        self.dialect = config.dialect;
        self
    }

    pub fn with_out_dir(mut self, out_dir: impl Into<PathBuf>) -> Self {
        self.out_dir = out_dir.into();
        self
    }

    pub fn with_dialect(mut self, dialect: SqlDialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Set the migration table name
    pub fn with_table_name(mut self, name: impl Into<String>) -> Self {
        self.table_name = name.into();
        self
    }

    /// Set the migration table schema
    pub fn with_table_schema(mut self, schema: impl Into<String>) -> Self {
        self.table_schema = Some(schema.into());
        self
    }

    /// Set the migration prefix style
    pub fn with_prefix(mut self, prefix: MigrationPrefix) -> Self {
        self.prefix = prefix;
        self
    }

    /// Ensure the migrations directory exists
    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.out_dir)?;
        // TODO: [snapshots] std::fs::create_dir_all(self.out_dir.join("_meta"))?;
        Ok(())
    }

    /// Generate the next migration name
    pub fn next_migration_name(&self, suffix: Option<impl ToString>) -> Result<String> {
        let existing = self.list_up_migrations()?;

        let suffix = match suffix {
            Some(s) => s.to_string(),
            None => petname::Petnames::default()
                .generate_one(2, "-")
                .expect("no names available")
                .to_lowercase(),
        };

        match self.prefix {
            MigrationPrefix::Index => {
                let next_idx = existing.len();
                Ok(format!("{:04}_{}", next_idx, suffix))
            }
            MigrationPrefix::Timestamp => {
                let ts = Utc::now().format("%Y%m%d%H%M%S").to_string();
                Ok(format!("{}_{}", ts, suffix))
            }
            MigrationPrefix::Unix => {
                let ts = Utc::now().timestamp();
                Ok(format!("{}_{}", ts, suffix))
            }
        }
    }

    /// Get a TableId for the migrations table, including schema if specified (PostgreSQL)
    pub fn migration_table(&self) -> TableId {
        (self.table_name.clone(), self.table_schema.clone()).into()
    }

    /// List all up migrations in the directory
    pub fn list_up_migrations(&self) -> Result<Vec<PathBuf>> {
        if !self.out_dir.exists() {
            return Ok(Vec::new());
        }

        let mut migrations: Vec<PathBuf> = std::fs::read_dir(&self.out_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                // Include .sql files but exclude .down.sql files
                filename.ends_with(".sql") && !filename.ends_with(".down.sql")
            })
            .map(|e| e.path())
            .collect();

        migrations.sort();
        Ok(migrations)
    }

    /// List all down migrations in the directory (files ending in .down.sql)
    pub fn list_down_migrations(&self) -> Result<Vec<PathBuf>> {
        if !self.out_dir.exists() {
            return Ok(Vec::new());
        }

        let mut migrations: Vec<PathBuf> = std::fs::read_dir(&self.out_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                filename.ends_with(".down.sql")
            })
            .map(|e| e.path())
            .collect();

        migrations.sort();
        Ok(migrations)
    }

    /// Get the down migration path for a given up migration
    pub fn get_down_migration_path(&self, up_migration: &Path) -> PathBuf {
        let stem = up_migration
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("migration");
        self.out_dir.join(format!("{}.down.sql", stem))
    }

    /// Check if a down migration exists for a given up migration
    pub fn has_down_migration(&self, up_migration: &Path) -> bool {
        self.get_down_migration_path(up_migration).exists()
    }

    /// Create the migrations table if it doesn't exist
    pub async fn ensure_migrations_table(&self, pool: &AnyPool) -> Result<()> {
        let query = queries::ensure_migrations(&self.dialect, &self.migration_table());
        sqlx::query(&query).execute(pool).await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to create migrations table '{}': {}",
                self.table_name, e
            ))
        })?;
        Ok(())
    }

    /// Get list of applied migrations from the database
    pub async fn get_applied_migrations(&self, pool: &AnyPool) -> Result<Vec<MigrationRow>> {
        self.ensure_migrations_table(pool).await?;
        let query = queries::select_migrations(&self.dialect, &self.migration_table());
        let rows = sqlx::query_as::<_, MigrationRow>(&query)
            .fetch_all(pool)
            .await?;
        Ok(rows)
    }

    /// Get pending migrations
    pub async fn get_pending_migrations(&self, pool: &AnyPool) -> Result<Vec<PathBuf>> {
        let all_migrations = self.list_up_migrations()?;
        let applied = self.get_applied_migrations(pool).await?;

        let applied_set: std::collections::HashSet<String> =
            applied.into_iter().map(|m| m.name).collect();

        let pending: Vec<PathBuf> = all_migrations
            .into_iter()
            .filter(|p| {
                let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                !applied_set.contains(name)
            })
            .collect();

        Ok(pending)
    }

    /// Validate checksums of applied migrations against the migration files
    ///
    /// Returns an error if any applied migration's checksum doesn't match the
    /// current file's checksum. This detects if migration files have been
    /// modified after being applied.
    ///
    /// Migrations that were applied before checksum tracking (with null checksums)
    /// are skipped.
    pub async fn validate_checksums(&self, pool: &AnyPool) -> Result<()> {
        let applied = self.get_applied_migrations(pool).await?;

        for migration in applied {
            // Skip migrations without checksums (applied before checksum tracking)
            let Some(stored_checksum) = migration.checksum else {
                continue;
            };

            // Find the migration file
            let migration_path = self.out_dir.join(format!("{}.sql", migration.name));
            if !migration_path.exists() {
                // Migration file not found - could be intentionally removed
                // We don't error here since the migration was already applied
                continue;
            }

            // Calculate current checksum
            let sql = std::fs::read_to_string(&migration_path)?;
            let current_checksum = sql_checksum(&sql);

            // Compare checksums
            if stored_checksum != current_checksum {
                return Err(ShkiError::checksum_mismatch(
                    &migration.name,
                    &stored_checksum,
                    &current_checksum,
                ));
            }
        }

        Ok(())
    }

    /// Apply a single migration within a transaction
    ///
    /// The entire migration (all statements) is executed within a single transaction.
    /// If any statement fails, the entire migration is rolled back.
    ///
    /// Note: Some statements like `CREATE INDEX CONCURRENTLY` in PostgreSQL cannot
    /// run inside a transaction. For such cases, use separate migration files.
    pub async fn apply_migration(&self, pool: &AnyPool, migration_path: &Path) -> Result<String> {
        self.ensure_migrations_table(pool).await?;

        let sql = std::fs::read_to_string(migration_path)?;
        let name = migration_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;

        // Calculate checksum of the SQL content
        let checksum = sql_checksum(&sql);

        // Execute all statements within a transaction
        let mut tx = pool.begin().await?;

        sqlx::raw_sql(&sql).execute(&mut *tx).await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to execute statement in migration '{}': {}\nStatement: {}",
                name,
                e,
                truncate_sql(&sql, 200)
            ))
        })?;

        // Build the SQL for recording the migration
        let query = queries::insert_migration(&self.dialect, &self.migration_table());

        // Record the migration with checksum within the same transaction
        sqlx::query(&query)
            .bind(name)
            .bind(&checksum)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to record migration '{}': {}", name, e))
            })?;

        // Commit the transaction
        tx.commit().await.map_err(|e| {
            ShkiError::migration(format!("Failed to commit migration '{}': {}", name, e))
        })?;

        Ok(checksum)
    }

    /// Record an existing migration file as applied without executing its SQL.
    ///
    /// This is useful for bootstrap/adoption workflows where the database already
    /// matches the migration state and only tracking metadata should be inserted.
    pub async fn mark_migration_applied(
        &self,
        pool: &AnyPool,
        migration_path: &Path,
    ) -> Result<()> {
        self.ensure_migrations_table(pool).await?;

        let sql = std::fs::read_to_string(migration_path)?;
        let name = migration_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;

        let checksum = sql_checksum(&sql);
        let query = queries::insert_migration(&self.dialect, &self.migration_table());

        sqlx::query(&query)
            .bind(name)
            .bind(&checksum)
            .execute(pool)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to record migration '{}': {}", name, e))
            })?;

        Ok(())
    }

    /// Apply all pending migrations
    ///
    /// Validates both snapshots and applied migration checksums before applying
    /// new ones. If any checksum mismatch is detected, the operation fails before
    /// any new migrations are applied.
    pub async fn apply_all(&self, pool: &AnyPool) -> Result<Vec<String>> {
        // Validate snapshots against migration files first
        // self.validate_snapshots()?;

        // Validate checksums of already-applied migrations
        self.validate_checksums(pool).await?;

        let pending = self.get_pending_migrations(pool).await?;
        let mut applied = Vec::new();

        for migration_path in pending {
            let name = migration_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            self.apply_migration(pool, &migration_path).await?;
            applied.push(name);
        }

        Ok(applied)
    }

    /// Get migrations that can be rolled back (applied migrations that have a down file)
    ///
    /// Returns migrations in reverse order (most recent first) that:
    /// 1. Have been applied to the database
    /// 2. Have a corresponding .down.sql file
    pub async fn get_rollback_migrations(&self, pool: &AnyPool) -> Result<Vec<PathBuf>> {
        let applied = self.get_applied_migrations(pool).await?;

        // Get down migrations that exist and match applied migrations
        // Return in reverse order (most recent first)
        let rollback_migrations: Vec<PathBuf> = applied
            .into_iter()
            .rev() // Most recent first
            .filter_map(|m| {
                let down_path = self.out_dir.join(format!("{}.down.sql", m.name));
                if down_path.exists() {
                    Some(down_path)
                } else {
                    None
                }
            })
            .collect();

        // Keep in reverse chronological order
        Ok(rollback_migrations)
    }

    /// Rollback a single migration using its down migration file
    ///
    /// Executes the down migration within a transaction and removes
    /// the migration record from the migrations table.
    pub async fn rollback_migration(
        &self,
        pool: &AnyPool,
        down_migration_path: &Path,
    ) -> Result<()> {
        let sql = std::fs::read_to_string(down_migration_path)?;

        // Extract the migration name from the down file (remove .down.sql)
        let filename = down_migration_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid down migration filename"))?;

        let name = filename
            .strip_suffix(".down.sql")
            .ok_or_else(|| ShkiError::migration("Down migration must end with .down.sql"))?;

        // Build the SQL for removing the migration record
        let delete_sql = queries::delete_table(&self.dialect, &self.migration_table());

        // Execute all statements within a transaction
        let mut tx = pool.begin().await?;

        sqlx::raw_sql(&sql).execute(&mut *tx).await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to execute statement in down migration '{}': {}\nStatement: {}",
                name,
                e,
                truncate_sql(&sql, 200)
            ))
        })?;

        // Remove the migration record within the same transaction
        sqlx::query(&delete_sql)
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!(
                    "Failed to remove migration record '{}': {}",
                    name, e
                ))
            })?;

        // Commit the transaction
        tx.commit().await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to commit rollback of migration '{}': {}",
                name, e
            ))
        })?;

        Ok(())
    }

    /// Rollback migrations until there are no more down migrations available
    ///
    /// This will rollback applied migrations in reverse order, stopping when:
    /// - All applied migrations with down files have been rolled back
    /// - An error occurs
    ///
    /// Returns the names of migrations that were rolled back.
    pub async fn rollback_all(&self, pool: &AnyPool) -> Result<Vec<String>> {
        let rollback_migrations = self.get_rollback_migrations(pool).await?;
        let mut rolled_back = Vec::new();

        for down_path in rollback_migrations {
            let name = down_path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".down.sql"))
                .unwrap_or("unknown")
                .to_string();

            self.rollback_migration(pool, &down_path).await?;
            rolled_back.push(name);
        }

        Ok(rolled_back)
    }

    /// Rollback a specific number of migrations
    ///
    /// Rolls back up to `count` migrations in reverse order.
    /// Only migrations with down files can be rolled back.
    pub async fn rollback_count(&self, pool: &AnyPool, count: usize) -> Result<Vec<String>> {
        let rollback_migrations = self.get_rollback_migrations(pool).await?;
        let mut rolled_back = Vec::new();

        for down_path in rollback_migrations.into_iter().take(count) {
            let name = down_path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".down.sql"))
                .unwrap_or("unknown")
                .to_string();

            self.rollback_migration(pool, &down_path).await?;
            rolled_back.push(name);
        }

        Ok(rolled_back)
    }

    /// Create a blank migration file for manual SQL editing
    ///
    /// This creates a new migration file with a template that users can fill in
    /// with their own SQL statements.
    ///
    /// # Arguments
    /// * `name` - A descriptive name for the migration (e.g., "add_user_index", "create_audit_table")
    ///
    /// # Returns
    /// The path to the created migration file
    pub fn create_blank_migration(&self, name: &str) -> Result<PathBuf> {
        self.create_blank_migration_with_down(name, false)
            .map(|(up_path, _)| up_path)
    }

    /// Create a blank migration file with optional down migration
    ///
    /// # Arguments
    /// * `name` - A descriptive name for the migration
    /// * `with_down` - Whether to also create a .down.sql file
    ///
    /// # Returns
    /// A tuple of (up_path, Option<down_path>)
    pub fn create_blank_migration_with_down(
        &self,
        name: &str,
        with_down: bool,
    ) -> Result<(PathBuf, Option<PathBuf>)> {
        self.ensure_dir()?;

        // Sanitize the name (replace spaces with underscores, remove special chars)
        let sanitized_name = sanitize_migration_name(name);

        // Generate migration name with prefix
        let migration_name = self.next_migration_name(Some(&sanitized_name))?;
        let up_path = self.out_dir.join(format!("{}.sql", migration_name));

        // Create up migration template content
        let up_content = generate_blank_migration_template(&migration_name, self.dialect, false);
        std::fs::write(&up_path, up_content)?;

        // Create down migration if requested
        let down_path = if with_down {
            let path = self.out_dir.join(format!("{}.down.sql", migration_name));
            let down_content =
                generate_blank_migration_template(&migration_name, self.dialect, true);
            std::fs::write(&path, down_content)?;
            Some(path)
        } else {
            None
        };

        Ok((up_path, down_path))
    }

    /// Create a blank migration with custom template content
    ///
    /// Similar to `create_blank_migration` but allows specifying initial SQL content.
    ///
    /// # Arguments
    /// * `name` - A descriptive name for the migration
    /// * `initial_sql` - Optional initial SQL to include in the up file
    ///
    /// # Returns
    /// The path to the created migration file
    pub fn create_blank_migration_with_content(
        &self,
        name: &str,
        initial_sql: Option<&str>,
    ) -> Result<PathBuf> {
        self.create_blank_migration_with_content_and_down(name, initial_sql, None)
            .map(|(up_path, _)| up_path)
    }

    /// Generate migration file content with header and optional SQL
    fn migration_content(
        &self,
        migration_name: &str,
        sql_content: Option<&str>,
        direction: MigrationDirection,
    ) -> String {
        let mut content = String::new();
        writeln!(
            &mut content,
            "-- Migration: {} ({})",
            migration_name, direction
        )
        .expect("writing to String cannot fail");
        writeln!(&mut content, "-- Created at: {}", Utc::now().to_rfc3339())
            .expect("writing to String cannot fail");
        content.push_str("-- Type: manual\n");
        content.push_str("--\n");
        content.push_str("-- This migration was created for manual editing.\n");
        content.push_str("-- The entire migration runs in a single transaction.\n");
        content.push_str("--\n");
        writeln!(
            &mut content,
            "-- Use '{}' to visually separate statements.",
            MIGRATION_SPLIT_MARKER
        )
        .expect("writing to String cannot fail");
        content.push('\n');

        if let Some(sql) = sql_content {
            content.push_str(sql);
            if !sql.ends_with('\n') {
                content.push('\n');
            }
        }

        content
    }

    /// Create a blank migration with custom content for both up and down
    ///
    /// # Arguments
    /// * `name` - A descriptive name for the migration
    /// * `up_sql` - Optional initial SQL for the up migration
    /// * `down_sql` - Optional initial SQL for the down migration (if Some, creates .down.sql)
    ///
    /// # Returns
    /// A tuple of (up_path, Option<down_path>)
    pub fn create_blank_migration_with_content_and_down(
        &self,
        name: &str,
        up_sql: Option<&str>,
        down_sql: Option<&str>,
    ) -> Result<(PathBuf, Option<PathBuf>)> {
        self.ensure_dir()?;

        let sanitized_name = sanitize_migration_name(name);
        let migration_name = self.next_migration_name(Some(&sanitized_name))?;
        let up_path = self.out_dir.join(format!("{}.sql", migration_name));

        // Write up migration
        let up_content = self.migration_content(&migration_name, up_sql, MigrationDirection::Up);

        std::fs::write(&up_path, up_content)?;

        // Write down migration if provided
        let down_path = if let Some(down) = down_sql {
            let path = self.out_dir.join(format!("{}.down.sql", migration_name));
            let down_content =
                self.migration_content(&migration_name, Some(down), MigrationDirection::Down);

            std::fs::write(&path, down_content)?;
            Some(path)
        } else {
            None
        };

        Ok((up_path, down_path))
    }
}

// ----------Snapshots-----------
// impl MigrationManager {
//     /// Load the latest snapshot
//     pub fn load_latest_snapshot(&self) -> Result<Option<Snapshot>> {
//         Snapshot::load_latest(&self.out_dir)
//     }
//
//     /// Create a new migration (up only)
//     pub fn create_migration(
//         &self,
//         name: Option<String>,
//         sql: &str,
//         from_snapshot: Option<&Snapshot>,
//         to_snapshot: &Snapshot,
//     ) -> Result<PathBuf> {
//         self.create_migration_with_down(name, sql, None, from_snapshot, to_snapshot)
//             .map(|(up_path, _)| up_path)
//     }
//
//     /// Create a new migration with optional down migration
//     ///
//     /// Returns a tuple of (up_migration_path, Option<down_migration_path>)
//     pub fn create_migration_with_down(
//         &self,
//         name: Option<String>,
//         up_sql: &str,
//         down_sql: Option<&str>,
//         from_snapshot: Option<&Snapshot>,
//         to_snapshot: &Snapshot,
//     ) -> Result<(PathBuf, Option<PathBuf>)> {
//         self.ensure_dir()?;
//
//         // Generate migration name
//         let name = self.next_migration_name(name)?;
//         let up_path = self.out_dir.join(format!("{}.sql", name));
//
//         // Write up migration SQL file
//         let mut up_content = String::new();
//         writeln!(&mut up_content, "-- Migration: {} (up)", name)
//             .expect("writing to String cannot fail");
//         writeln!(
//             &mut up_content,
//             "-- Created at: {}",
//             Utc::now().to_rfc3339()
//         )
//         .expect("writing to String cannot fail");
//         if let Some(from) = from_snapshot {
//             writeln!(&mut up_content, "-- From snapshot: {}", from.id)
//                 .expect("writing to String cannot fail");
//         }
//         writeln!(&mut up_content, "-- To snapshot: {}", to_snapshot.id)
//             .expect("writing to String cannot fail");
//         up_content.push('\n');
//         up_content.push_str(up_sql);
//
//         std::fs::write(&up_path, &up_content)?;
//
//         // Write down migration if provided
//         let down_path = if let Some(down) = down_sql {
//             let path = self.out_dir.join(format!("{}.down.sql", name));
//
//             let mut down_content = String::new();
//             writeln!(&mut down_content, "-- Migration: {} (down)", name)
//                 .expect("writing to String cannot fail");
//             writeln!(
//                 &mut down_content,
//                 "-- Created at: {}",
//                 Utc::now().to_rfc3339()
//             )
//             .expect("writing to String cannot fail");
//             down_content
//                 .push_str("-- This migration reverses the changes made by the up migration.\n");
//             down_content.push('\n');
//             down_content.push_str(down);
//
//             std::fs::write(&path, down_content)?;
//             Some(path)
//         } else {
//             None
//         };
//
//         // Save the new snapshot with migration metadata
//         // Use the full file content checksum so it matches what gets stored when applied.
//         let mut snapshot_with_migration = to_snapshot
//             .clone()
//             .with_migration(name.clone(), sql_checksum(&up_content));
//
//         if snapshot_with_migration.prev_id.is_none() {
//             snapshot_with_migration.prev_id = from_snapshot.map(|s| s.id.clone());
//         }
//
//         snapshot_with_migration.save(&self.out_dir)?;
//
//         Ok((up_path, down_path))
//     }
//
//     /// Save a post-migration snapshot with migration metadata.
//     ///
//     /// Used for manual migrations where the schema state is introspected
//     /// after applying SQL.
//     pub fn save_post_migration_snapshot(
//         &self,
//         mut snapshot: Snapshot,
//         migration_name: &str,
//         migration_checksum: &str,
//     ) -> Result<PathBuf> {
//         self.remove_snapshots_for_migration(migration_name)?;
//
//         if snapshot.prev_id.is_none() {
//             snapshot.prev_id = self.load_latest_snapshot()?.map(|s| s.id);
//         }
//
//         let snapshot = snapshot.with_migration(migration_name, migration_checksum);
//         snapshot.save(&self.out_dir)
//     }
//
//     /// Remove all snapshots linked to a migration name.
//     ///
//     /// Returns the number of snapshot files removed.
//     pub fn remove_snapshots_for_migration(&self, migration_name: &str) -> Result<usize> {
//         let meta_dir = self.out_dir.join("_meta");
//         if !meta_dir.exists() {
//             return Ok(0);
//         }
//
//         let mut removed = 0usize;
//
//         for entry in std::fs::read_dir(&meta_dir)? {
//             let entry = entry?;
//             let path = entry.path();
//             if path.extension().and_then(|e| e.to_str()) != Some("json") {
//                 continue;
//             }
//
//             let content = std::fs::read_to_string(&path)?;
//             let parsed = Snapshot::from_json(&content)?;
//             let is_match = parsed
//                 .migration
//                 .as_ref()
//                 .map(|m| m.name == migration_name)
//                 .unwrap_or(false);
//
//             if is_match {
//                 std::fs::remove_file(path)?;
//                 removed += 1;
//             }
//         }
//
//         Ok(removed)
//     }
//
//     /// Validate snapshots against their associated migration files
//     ///
//     /// Checks that each snapshot's stored migration checksum matches the
//     /// current checksum of the corresponding migration file. This detects
//     /// if migration files have been modified after the snapshots were created.
//     ///
//     /// Returns an error with a detailed summary if any mismatches are found.
//     pub fn validate_snapshots(&self) -> Result<()> {
//         let snapshots = Snapshot::load_all(&self.out_dir)?;
//
//         let total_snapshots = snapshots.len();
//         let mut snapshots_with_migrations = 0;
//         let mut mismatches = Vec::new();
//
//         for snapshot in snapshots {
//             // Skip snapshots without migration info
//             let Some(ref migration_info) = snapshot.migration else {
//                 continue;
//             };
//
//             snapshots_with_migrations += 1;
//
//             // Find the migration file
//             let migration_path = self.out_dir.join(format!("{}.sql", migration_info.name));
//
//             if !migration_path.exists() {
//                 mismatches.push(MismatchDetail {
//                     snapshot_id: snapshot.id.clone(),
//                     migration_name: migration_info.name.clone(),
//                     snapshot_checksum: migration_info.checksum.clone(),
//                     file_checksum: None,
//                     issue: "Migration file not found".to_string(),
//                 });
//                 continue;
//             }
//
//             // Calculate current checksum
//             let sql = std::fs::read_to_string(&migration_path)?;
//             let current_checksum = sql_checksum(&sql);
//
//             // Compare checksums
//             if migration_info.checksum != current_checksum {
//                 mismatches.push(MismatchDetail {
//                     snapshot_id: snapshot.id.clone(),
//                     migration_name: migration_info.name.clone(),
//                     snapshot_checksum: migration_info.checksum.clone(),
//                     file_checksum: Some(current_checksum),
//                     issue: "Checksum mismatch - migration file has been modified".to_string(),
//                 });
//             }
//         }
//
//         if !mismatches.is_empty() {
//             return Err(ShkiError::snapshot_validation(SnapshotValidationSummary {
//                 total_snapshots,
//                 snapshots_with_migrations,
//                 mismatches,
//             }));
//         }
//
//         Ok(())
//     }
//
//     /// Find applied migrations that don't have corresponding snapshots
//     ///
//     /// Returns a list of migration names that exist in the database but don't
//     /// have a snapshot with matching migration info and a list of migrations in the
//     /// DB that have matching checksums to snapshots.
//     ///
//     /// Checksum matching can indicate that the name of the migration has changed, but the
//     /// sql content in the migration is still the same.
//     ///
//     /// This is really just a nice to have and any actual resolution will require
//     /// manual intervention to ensure data integrity
//     pub async fn find_migrations_without_snapshots(
//         &self,
//         pool: &AnyPool,
//     ) -> Result<(Vec<MigrationRow>, Vec<MigrationRow>, Vec<(String, String)>)> {
//         let applied = self.get_applied_migrations(pool).await?;
//         let snapshots = Snapshot::load_all(&self.out_dir)?;
//
//         // Build a set of migration names that have snapshots
//         let snapshot_names: std::collections::HashSet<String> = snapshots
//             .iter()
//             .filter_map(|s| s.migration.as_ref())
//             .map(|m| m.name.clone())
//             .collect();
//
//         // Find applied migrations without snapshots
//         let missing: Vec<MigrationRow> = applied
//             .iter()
//             .filter(|m| !snapshot_names.contains(&m.name))
//             .cloned()
//             .collect();
//
//         let snapshots_by_checksum: HashMap<&str, &str> = snapshots
//             .iter()
//             .filter_map(|s| s.migration.as_ref())
//             .map(|m| (m.checksum.as_str(), m.name.as_str()))
//             .collect();
//
//         // get (migration, snapshot) names where checksums match
//         let checksums_match = missing
//             .iter()
//             .filter_map(|m| {
//                 let checksum = m.checksum.as_deref()?;
//                 let snapshot_name = snapshots_by_checksum.get(checksum)?;
//                 Some((m.name.clone(), (*snapshot_name).to_owned()))
//             })
//             .collect();
//
//         Ok((applied, missing, checksums_match))
//     }
//
// /// Ensure every applied migration has a corresponding snapshot entry.
// pub async fn ensure_snapshot_coverage(&self, pool: &AnyPool) -> Result<()> {
//     let (_applied, missing, _checksums_match) =
//         self.find_migrations_without_snapshots(pool).await?;
//
//     if missing.is_empty() {
//         return Ok(());
//     }
//
//     let missing_names = missing
//         .iter()
//         .map(|m| m.name.as_str())
//         .collect::<Vec<_>>()
//         .join(", ");
//
//     Err(ShkiError::validation(format!(
//         "Applied migrations missing snapshots: {}. Each applied migration must have a snapshot in migrations/_meta. Run `shki status` for details.",
//         missing_names
//     )))
// }
// }
