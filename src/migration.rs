//! Migration file management
//!
//! This module handles reading, writing, and applying migration files.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use petname::Generator;
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;
use sqlx::prelude::FromRow;

use crate::config::MigrationPrefix;
use crate::schema::SchemaDialect;
use crate::snapshot::Snapshot;
use crate::{Result, ShkiError};

use crate::queries;

pub const MIGRATION_SPLIT_MARKER: &str = "--> +statement";

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
    pub dialect: SchemaDialect,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MigrationRow {
    pub id: i64,
    pub name: String,
    pub applied_at: String,
}

impl MigrationManager {
    /// Create a new migration manager
    pub fn new(out_dir: impl Into<PathBuf>, dialect: SchemaDialect) -> Self {
        Self {
            out_dir: out_dir.into(),
            table_name: "__shki_migrations".to_string(),
            table_schema: None,
            prefix: MigrationPrefix::Index,
            dialect,
        }
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
        std::fs::create_dir_all(self.out_dir.join("_meta"))?;
        Ok(())
    }

    /// Generate the next migration name
    pub fn next_migration_name(&self, suffix: Option<impl ToString>) -> Result<String> {
        let existing = self.list_migrations()?;

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
                dbg!(&ts);
                Ok(format!("{}_{}", ts, suffix))
            }
            MigrationPrefix::Unix => {
                let ts = Utc::now().timestamp();
                Ok(format!("{}_{}", ts, suffix))
            }
        }
    }

    /// List all up migrations in the directory (files ending in .sql but not .down.sql)
    pub fn list_migrations(&self) -> Result<Vec<PathBuf>> {
        self.list_up_migrations()
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

    /// Create a new migration (up only)
    pub fn create_migration(
        &self,
        name: Option<String>,
        sql: &str,
        from_snapshot: Option<&Snapshot>,
        to_snapshot: &Snapshot,
    ) -> Result<PathBuf> {
        self.create_migration_with_down(name, sql, None, from_snapshot, to_snapshot)
            .map(|(up_path, _)| up_path)
    }

    /// Create a new migration with optional down migration
    ///
    /// Returns a tuple of (up_migration_path, Option<down_migration_path>)
    pub fn create_migration_with_down(
        &self,
        name: Option<String>,
        up_sql: &str,
        down_sql: Option<&str>,
        from_snapshot: Option<&Snapshot>,
        to_snapshot: &Snapshot,
    ) -> Result<(PathBuf, Option<PathBuf>)> {
        self.ensure_dir()?;

        // Generate migration name
        let name = self.next_migration_name(name)?;
        let up_path = self.out_dir.join(format!("{}.sql", name));

        // Write up migration SQL file
        let mut up_content = String::new();
        up_content.push_str(&format!("-- Migration: {} (up)\n", name));
        up_content.push_str(&format!("-- Created at: {}\n", Utc::now().to_rfc3339()));
        if let Some(from) = from_snapshot {
            up_content.push_str(&format!("-- From snapshot: {}\n", from.id));
        }
        up_content.push_str(&format!("-- To snapshot: {}\n", to_snapshot.id));
        up_content.push('\n');
        up_content.push_str(up_sql);

        std::fs::write(&up_path, up_content)?;

        // Write down migration if provided
        let down_path = if let Some(down) = down_sql {
            let path = self.out_dir.join(format!("{}.down.sql", name));

            let mut down_content = String::new();
            down_content.push_str(&format!("-- Migration: {} (down)\n", name));
            down_content.push_str(&format!("-- Created at: {}\n", Utc::now().to_rfc3339()));
            down_content
                .push_str("-- This migration reverses the changes made by the up migration.\n");
            down_content.push('\n');
            down_content.push_str(down);

            std::fs::write(&path, down_content)?;
            Some(path)
        } else {
            None
        };

        // Save the new snapshot
        to_snapshot.save(&self.out_dir)?;

        Ok((up_path, down_path))
    }

    /// Create the migrations table if it doesn't exist
    pub async fn ensure_migrations_table(&self, pool: &AnyPool) -> Result<()> {
        let query = queries::ensure_migrations(&self.dialect, &self.table_schema, &self.table_name);
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
        let query = queries::select_migrations(&self.dialect, &self.table_schema, &self.table_name);
        let rows = sqlx::query_as::<_, MigrationRow>(&query)
            .fetch_all(pool)
            .await?;
        Ok(rows)
    }

    /// Get pending migrations
    pub async fn get_pending_migrations(&self, pool: &AnyPool) -> Result<Vec<PathBuf>> {
        let all_migrations = self.list_migrations()?;
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

    /// Apply a single migration within a transaction
    ///
    /// The entire migration (all statements) is executed within a single transaction.
    /// If any statement fails, the entire migration is rolled back.
    ///
    /// Note: Some statements like `CREATE INDEX CONCURRENTLY` in PostgreSQL cannot
    /// run inside a transaction. For such cases, use separate migration files.
    pub async fn apply_migration(&self, pool: &AnyPool, migration_path: &Path) -> Result<()> {
        let sql = std::fs::read_to_string(migration_path)?;
        let name = migration_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?;

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
        let query = queries::insert_migration(&self.dialect, &self.table_schema, &self.table_name);

        // Record the migration within the same transaction
        sqlx::query(&query)
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                ShkiError::migration(format!("Failed to record migration '{}': {}", name, e))
            })?;

        // Commit the transaction
        tx.commit().await.map_err(|e| {
            ShkiError::migration(format!("Failed to commit migration '{}': {}", name, e))
        })?;

        Ok(())
    }

    /// Apply all pending migrations
    pub async fn apply_all(&self, pool: &AnyPool) -> Result<Vec<String>> {
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
        let delete_sql = queries::delete_table(&self.dialect, &self.table_schema, &self.table_name);

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

    /// Load the latest snapshot
    pub fn load_latest_snapshot(&self) -> Result<Option<Snapshot>> {
        Snapshot::load_latest(&self.out_dir)
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
        content.push_str(&format!(
            "-- Migration: {} ({})\n",
            migration_name, direction
        ));
        content.push_str(&format!("-- Created at: {}\n", Utc::now().to_rfc3339()));
        content.push_str("-- Type: manual\n");
        content.push_str("--\n");
        content.push_str("-- This migration was created for manual editing.\n");
        content.push_str("-- The entire migration runs in a single transaction.\n");
        content.push_str("--\n");
        content.push_str(&format!(
            "-- Use '{}' to visually separate statements.\n",
            MIGRATION_SPLIT_MARKER
        ));
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

/// Sanitize a migration name to be filesystem-safe
fn sanitize_migration_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        // Remove consecutive underscores
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Generate a blank migration template with helpful comments
///
/// # Arguments
/// * `migration_name` - The name of the migration
/// * `dialect` - The database dialect
/// * `is_down` - Whether this is a down migration template
fn generate_blank_migration_template(
    migration_name: &str,
    dialect: SchemaDialect,
    is_down: bool,
) -> String {
    let mut content = String::new();

    let direction = if is_down { "down" } else { "up" };
    content.push_str(&format!(
        "-- Migration: {} ({})\n",
        migration_name, direction
    ));
    content.push_str(&format!("-- Created at: {}\n", Utc::now().to_rfc3339()));
    content.push_str("-- Type: manual\n");
    content.push_str("--\n");

    if is_down {
        content.push_str("-- This migration reverses the changes made by the up migration.\n");
    } else {
        content.push_str("-- This migration was created for manual editing.\n");
    }

    content.push_str("-- Write your SQL statements below.\n");
    content.push_str("--\n");
    content.push_str("-- Tips:\n");
    content.push_str("-- - The entire migration runs in a single transaction\n");
    content.push_str("-- - If any statement fails, all changes are rolled back\n");
    content.push_str(&format!(
        "-- - Use '{}' to visually separate statements\n",
        MIGRATION_SPLIT_MARKER
    ));
    content.push_str("-- - Remove these comments before committing\n");
    content.push_str("--\n");

    // Add dialect-specific examples
    if is_down {
        match dialect {
            SchemaDialect::Postgres => {
                content.push_str("-- Example PostgreSQL rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX CONCURRENTLY IF EXISTS idx_users_email;\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- ALTER TABLE posts DROP COLUMN IF EXISTS view_count;\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- DROP TYPE IF EXISTS status_type;\n");
            }
            SchemaDialect::Mysql => {
                content.push_str("-- Example MySQL rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX idx_users_email ON users;\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- ALTER TABLE posts DROP COLUMN view_count;\n");
            }
            SchemaDialect::Sqlite => {
                content.push_str("-- Example SQLite rollback statements:\n");
                content.push_str("--\n");
                content.push_str("-- DROP INDEX IF EXISTS idx_users_email;\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- Note: SQLite doesn't support DROP COLUMN directly.\n");
                content.push_str("-- You may need to recreate the table without the column.\n");
            }
        }
    } else {
        match dialect {
            SchemaDialect::Postgres => {
                content.push_str("-- Example PostgreSQL statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX CONCURRENTLY idx_users_email ON users(email);\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INTEGER DEFAULT 0;\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- CREATE TYPE status_type AS ENUM ('active', 'inactive');\n");
            }
            SchemaDialect::Mysql => {
                content.push_str("-- Example MySQL statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX idx_users_email ON users(email);\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INT DEFAULT 0;\n");
            }
            SchemaDialect::Sqlite => {
                content.push_str("-- Example SQLite statements:\n");
                content.push_str("--\n");
                content.push_str("-- CREATE INDEX idx_users_email ON users(email);\n");
                content.push_str(&format!("-- {}\n", MIGRATION_SPLIT_MARKER));
                content.push_str("-- ALTER TABLE posts ADD COLUMN view_count INTEGER DEFAULT 0;\n");
            }
        }
    }

    content.push_str("\n\n-- Write your SQL below this line:\n\n");

    content
}

/// Read a migration file and extract its SQL
pub fn read_migration(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;

    // Remove header comments
    let lines: Vec<&str> = content
        .lines()
        .skip_while(|line| line.starts_with("--") || line.is_empty())
        .collect();

    Ok(lines.join("\n"))
}

/// Truncate a SQL statement for display in error messages
fn truncate_sql(sql: &str, max_len: usize) -> String {
    let normalized: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.len() <= max_len {
        normalized
    } else {
        format!("{}...", &normalized[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_migration_name() {
        let cases = vec![
            ("Add User Index", "add-user-index"),
            ("create-audit-table", "create-audit-table"),
            ("Update@Schema#V2!**", "update-schema-v2"),
            ("   Leading and trailing   ", "leading-and-trailing"),
            ("Multiple   Spaces", "multiple-spaces"),
            ("Special$$$Chars%%%Here", "special-chars-here"),
        ];

        for (input, expected) in cases {
            let sanitized = super::sanitize_migration_name(input);
            assert_eq!(sanitized, expected);
        }
    }

    // ==================== MigrationManager Tests ====================

    #[test]
    fn test_migration_manager_new() {
        let manager = MigrationManager::new("/tmp/migrations", SchemaDialect::Postgres);
        assert_eq!(manager.out_dir, PathBuf::from("/tmp/migrations"));
        assert_eq!(manager.table_name, "__shki_migrations");
        assert!(manager.table_schema.is_none());
        assert_eq!(manager.dialect, SchemaDialect::Postgres);
    }

    #[test]
    fn test_migration_manager_with_table_name() {
        let manager = MigrationManager::new("/tmp/migrations", SchemaDialect::Postgres)
            .with_table_name("custom_migrations");
        assert_eq!(manager.table_name, "custom_migrations");
    }

    #[test]
    fn test_migration_manager_with_table_schema() {
        let manager = MigrationManager::new("/tmp/migrations", SchemaDialect::Postgres)
            .with_table_schema("myschema");
        assert_eq!(manager.table_schema, Some("myschema".to_string()));
    }

    #[test]
    fn test_migration_manager_with_prefix() {
        let manager = MigrationManager::new("/tmp/migrations", SchemaDialect::Postgres)
            .with_prefix(MigrationPrefix::Timestamp);
        assert_eq!(manager.prefix, MigrationPrefix::Timestamp);
    }

    #[test]
    fn test_list_migrations_empty_dir() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres);

        let migrations = manager
            .list_migrations()
            .expect("failed to list migrations");
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_list_migrations_nonexistent_dir() {
        let manager = MigrationManager::new("/nonexistent/path", SchemaDialect::Postgres);

        let migrations = manager
            .list_migrations()
            .expect("failed to list migrations");
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_list_migrations_arbitrary_sql_files() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        // Create arbitrary SQL migration files (like from another tool)
        fs::write(
            dir_path.join("001_create_users.sql"),
            "CREATE TABLE users (id INT);",
        )
        .expect("failed to write 001_create_users.sql");
        fs::write(
            dir_path.join("002_add_email.sql"),
            "ALTER TABLE users ADD COLUMN email TEXT;",
        )
        .expect("failed to write 002_add_email.sql");
        fs::write(
            dir_path.join("003_create_posts.sql"),
            "CREATE TABLE posts (id INT);",
        )
        .expect("failed to write 003_create_posts.sql");

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres);
        let migrations = manager
            .list_migrations()
            .expect("failed to list migrations");

        assert_eq!(migrations.len(), 3);

        // Verify they are sorted
        let names: Vec<_> = migrations
            .iter()
            .map(|p| {
                p.file_name()
                    .expect("migration path missing filename")
                    .to_str()
                    .expect("filename contains invalid UTF-8")
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "001_create_users.sql",
                "002_add_email.sql",
                "003_create_posts.sql"
            ]
        );
    }

    #[test]
    fn test_list_migrations_excludes_down_files() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        // Create up and down migrations
        fs::write(
            dir_path.join("001_initial.sql"),
            "CREATE TABLE users (id INT);",
        )
        .expect("failed to write 001_initial.sql");
        fs::write(dir_path.join("001_initial.down.sql"), "DROP TABLE users;")
            .expect("failed to write 001_initial.down.sql");
        fs::write(
            dir_path.join("002_add_email.sql"),
            "ALTER TABLE users ADD COLUMN email TEXT;",
        )
        .expect("failed to write 002_add_email.sql");
        fs::write(
            dir_path.join("002_add_email.down.sql"),
            "ALTER TABLE users DROP COLUMN email;",
        )
        .expect("failed to write 002_add_email.down.sql");

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres);
        let up_migrations = manager
            .list_up_migrations()
            .expect("failed to list up migrations");
        let down_migrations = manager
            .list_down_migrations()
            .expect("failed to list down migrations");

        assert_eq!(up_migrations.len(), 2);
        assert_eq!(down_migrations.len(), 2);

        // Verify up migrations don't include .down.sql files
        for path in &up_migrations {
            assert!(
                !path
                    .to_str()
                    .expect("path contains invalid UTF-8")
                    .ends_with(".down.sql")
            );
        }

        // Verify down migrations only include .down.sql files
        for path in &down_migrations {
            assert!(
                path.to_str()
                    .expect("path contains invalid UTF-8")
                    .ends_with(".down.sql")
            );
        }
    }

    #[test]
    fn test_list_migrations_ignores_non_sql_files() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        // Create various files
        fs::write(
            dir_path.join("001_migration.sql"),
            "CREATE TABLE t1 (id INT);",
        )
        .expect("failed to write 001_migration.sql");
        fs::write(dir_path.join("README.md"), "# Migrations").expect("failed to write README.md");
        fs::write(dir_path.join("config.json"), "{}").expect("failed to write config.json");
        fs::write(dir_path.join(".gitkeep"), "").expect("failed to write .gitkeep");
        fs::create_dir(dir_path.join("_meta")).expect("failed to create _meta directory");
        fs::write(dir_path.join("_meta/snapshot.json"), "{}")
            .expect("failed to write _meta/snapshot.json");

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres);
        let migrations = manager
            .list_migrations()
            .expect("failed to list migrations");

        // Only the .sql file should be listed
        assert_eq!(migrations.len(), 1);
        assert!(
            migrations[0]
                .file_name()
                .expect("migration path missing filename")
                .to_str()
                .expect("filename contains invalid UTF-8")
                .ends_with(".sql")
        );
    }

    #[test]
    fn test_list_migrations_various_naming_formats() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        // Test various naming conventions from different tools
        fs::write(dir_path.join("20231215120000_initial.sql"), "SELECT 1;")
            .expect("failed to write timestamp-prefixed migration");
        fs::write(dir_path.join("V1__initial.sql"), "SELECT 2;")
            .expect("failed to write flyway-style migration");
        fs::write(dir_path.join("0001_first.sql"), "SELECT 3;")
            .expect("failed to write index-prefixed migration");
        fs::write(dir_path.join("custom_migration.sql"), "SELECT 4;")
            .expect("failed to write custom migration");

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres);
        let migrations = manager
            .list_migrations()
            .expect("failed to list migrations");

        // All should be listed and sorted alphabetically
        assert_eq!(migrations.len(), 4);
    }

    #[test]
    fn test_get_down_migration_path() {
        let manager = MigrationManager::new("/migrations", SchemaDialect::Postgres);

        let up_path = PathBuf::from("/migrations/001_initial.sql");
        let down_path = manager.get_down_migration_path(&up_path);

        assert_eq!(down_path, PathBuf::from("/migrations/001_initial.down.sql"));
    }

    #[test]
    fn test_has_down_migration() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        fs::write(dir_path.join("001_with_down.sql"), "CREATE TABLE t;")
            .expect("failed to write 001_with_down.sql");
        fs::write(dir_path.join("001_with_down.down.sql"), "DROP TABLE t;")
            .expect("failed to write 001_with_down.down.sql");
        fs::write(dir_path.join("002_without_down.sql"), "CREATE TABLE t2;")
            .expect("failed to write 002_without_down.sql");

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres);

        assert!(manager.has_down_migration(&dir_path.join("001_with_down.sql")));
        assert!(!manager.has_down_migration(&dir_path.join("002_without_down.sql")));
    }

    #[test]
    fn test_next_migration_with_prefix_index() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres)
            .with_prefix(MigrationPrefix::Index);

        // First migration should be 0000
        let name = manager
            .next_migration_name(Some("initial"))
            .expect("failed to get next migration name");
        assert_eq!(name, "0000_initial");

        // Add a migration file
        fs::write(dir_path.join("0000_initial.sql"), "SELECT 1;")
            .expect("failed to write 0000_initial.sql");

        // Next should be 0001
        let name = manager
            .next_migration_name(Some("add_users"))
            .expect("failed to get next migration name");
        assert_eq!(name, "0001_add_users");
    }

    #[test]
    fn test_next_migration_with_prefix_timestamp() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();

        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres)
            .with_prefix(MigrationPrefix::Timestamp);

        let name = manager
            .next_migration_name(Some("initial"))
            .expect("failed to get next migration name");

        // e.g. (20260125155257, "initial")
        let (date, file_name) = name
            .split_once('_')
            .expect("migration name missing underscore separator");

        // Verify date is a valid timestamp
        let init_date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M%S")
            .expect("failed to parse initial timestamp");
        assert_eq!(file_name, "initial");

        fs::write(dir_path.join(format!("{}_initial.sql", date)), "SELECT 1;")
            .expect("failed to write initial migration file");

        // Next should be a new date after or equal the previous one (only 1 second resolution)
        let name = manager
            .next_migration_name(Some("add_users"))
            .expect("failed to get next migration name");
        let (next_date, file_name) = name
            .split_once('_')
            .expect("migration name missing underscore separator");
        let next_date = NaiveDateTime::parse_from_str(next_date, "%Y%m%d%H%M%S")
            .expect("failed to parse next timestamp");

        // check for equality here because of test speed and don't want to rely on sleep
        assert!(next_date >= init_date);
        assert_eq!(file_name, "add_users");
    }

    #[test]
    fn test_next_migration_with_prefix_unix() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let dir_path = temp_dir.path();
        let manager = MigrationManager::new(dir_path, SchemaDialect::Postgres)
            .with_prefix(MigrationPrefix::Unix);
        let name = manager
            .next_migration_name(Some("initial"))
            .expect("failed to get next migration name");
        let (date, file_name) = name
            .split_once('_')
            .expect("migration name missing underscore separator");
        let init_date = DateTime::from_timestamp(
            date.parse::<i64>()
                .expect("timestamp should be valid number"),
            0,
        )
        .expect("failed to parse initial timestamp");
        assert_eq!(file_name, "initial");

        fs::write(dir_path.join(format!("{}_initial.sql", date)), "SELECT 1;")
            .expect("failed to write initial migration file");
        let name = manager
            .next_migration_name(Some("add_users"))
            .expect("failed to get next migration name");
        let (next_date, file_name) = name
            .split_once('_')
            .expect("migration name missing underscore separator");
        let next_date = DateTime::from_timestamp(
            next_date
                .parse::<i64>()
                .expect("timestamp should be valid number"),
            0,
        )
        .expect("failed to parse next timestamp");
        assert!(next_date >= init_date);
        assert_eq!(file_name, "add_users");
    }

    #[test]
    fn test_create_blank_migration() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres);

        let path = manager
            .create_blank_migration("add_index")
            .expect("failed to create blank migration");

        assert!(path.exists());
        assert!(
            path.file_name()
                .expect("migration path missing filename")
                .to_str()
                .expect("filename contains invalid UTF-8")
                .contains("add-index")
        );
        assert!(
            path.file_name()
                .expect("migration path missing filename")
                .to_str()
                .expect("filename contains invalid UTF-8")
                .ends_with(".sql")
        );

        let content = fs::read_to_string(&path).expect("failed to read migration file");
        assert!(content.contains("Migration:"));
        assert!(content.contains("manual"));
    }

    #[test]
    fn test_create_blank_migration_with_down() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres);

        let (up_path, down_path) = manager
            .create_blank_migration_with_down("add_column", true)
            .expect("failed to create migration with down file");

        assert!(up_path.exists());
        assert!(down_path.is_some());
        assert!(
            down_path
                .as_ref()
                .expect("down path should be Some")
                .exists()
        );

        let down_content = fs::read_to_string(down_path.expect("down path should be Some"))
            .expect("failed to read down migration file");
        assert!(down_content.contains("down"));
        assert!(down_content.contains("reverses"));
    }

    #[test]
    fn test_create_blank_migration_with_content() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres);

        let initial_sql = "CREATE INDEX idx_users_email ON users(email);";
        let path = manager
            .create_blank_migration_with_content("add_index", Some(initial_sql))
            .expect("failed to create migration with content");

        let content = fs::read_to_string(&path).expect("failed to read migration file");
        assert!(content.contains(initial_sql));
    }

    #[test]
    fn test_read_migration() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let path = temp_dir.path().join("001_test.sql");

        let content = r#"-- Migration: 001_test
-- Created at: 2024-01-01
-- Some comment

CREATE TABLE users (id INT);
ALTER TABLE users ADD COLUMN name TEXT;
"#;
        fs::write(&path, content).expect("failed to write test migration file");

        let sql = read_migration(&path).expect("failed to read migration");

        // Should skip header comments
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("ALTER TABLE"));
    }

    #[test]
    fn test_truncate_sql() {
        let short = "SELECT * FROM users";
        assert_eq!(truncate_sql(short, 50), short);

        let long = "SELECT id, name, email, created_at, updated_at FROM users WHERE is_active = true AND deleted_at IS NULL ORDER BY created_at DESC";
        let truncated = truncate_sql(long, 50);
        assert!(truncated.len() <= 53); // 50 + "..."
        assert!(truncated.ends_with("..."));
    }

    // ==================== Dialect-specific Tests ====================

    #[test]
    fn test_migration_manager_postgres_dialect() {
        let manager = MigrationManager::new("/tmp", SchemaDialect::Postgres);
        assert_eq!(manager.dialect, SchemaDialect::Postgres);
    }

    #[test]
    fn test_migration_manager_mysql_dialect() {
        let manager = MigrationManager::new("/tmp", SchemaDialect::Mysql);
        assert_eq!(manager.dialect, SchemaDialect::Mysql);
    }

    #[test]
    fn test_migration_manager_sqlite_dialect() {
        let manager = MigrationManager::new("/tmp", SchemaDialect::Sqlite);
        assert_eq!(manager.dialect, SchemaDialect::Sqlite);
    }

    #[test]
    fn test_blank_migration_template_postgres() {
        let template =
            generate_blank_migration_template("001_test", SchemaDialect::Postgres, false);
        assert!(template.contains("PostgreSQL"));
        assert!(template.contains("CREATE INDEX CONCURRENTLY"));
    }

    #[test]
    fn test_blank_migration_template_mysql() {
        let template = generate_blank_migration_template("001_test", SchemaDialect::Mysql, false);
        assert!(template.contains("MySQL"));
    }

    #[test]
    fn test_blank_migration_template_sqlite() {
        let template = generate_blank_migration_template("001_test", SchemaDialect::Sqlite, false);
        assert!(template.contains("SQLite"));
    }

    #[test]
    fn test_blank_down_migration_template() {
        let template = generate_blank_migration_template("001_test", SchemaDialect::Postgres, true);
        assert!(template.contains("down"));
        assert!(template.contains("reverses"));
        assert!(template.contains("DROP"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_migration_name_special_characters() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres);

        // Special characters should be sanitized
        let path = manager
            .create_blank_migration("Add User's Email Index!!!")
            .expect("failed to create migration with special characters");
        let filename = path
            .file_name()
            .expect("migration path missing filename")
            .to_str()
            .expect("filename contains invalid UTF-8");

        // Should not contain special characters
        assert!(!filename.contains("'"));
        assert!(!filename.contains("!"));
    }

    #[test]
    fn test_ensure_dir_creates_directories() {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let migrations_path = temp_dir.path().join("nested/migrations");

        let manager = MigrationManager::new(&migrations_path, SchemaDialect::Postgres);
        manager
            .ensure_dir()
            .expect("failed to ensure migrations directory");

        assert!(migrations_path.exists());
        assert!(migrations_path.join("_meta").exists());
    }
}
