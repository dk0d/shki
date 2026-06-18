//! Reading, writing, and applying migration files.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::checksum::sql_checksum;
use super::journal::{Journal, MigrationKind, journal_path};
use super::utils::{generate_blank_migration_template, sanitize_migration_name};
use crate::config::MigrationPrefix;
use crate::engines::Engine;
use crate::models::iden::Iden;
use crate::snapshots::Snapshot;
use crate::{MIGRATION_SPLIT_MARKER, Result, ShkiError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, tabled::Tabled)]
#[serde(rename_all = "snake_case")]
pub struct MigrationInfo {
    pub name: String,

    #[tabled(display("option_truncate", "", 5))]
    pub checksum: Option<String>,
}

/// Migration manager
pub struct MigrationManager {
    /// Output directory
    pub out_dir: PathBuf,

    /// Migration table
    pub table: Iden,

    /// Migration prefix style
    pub prefix: MigrationPrefix,

    /// Database dialect
    pub engine: Engine,
}

#[derive(Debug, Clone, Serialize, Deserialize, clap::Subcommand, Default)]
pub enum ApplyMigrationMode {
    /// Apply all migrations (default)
    #[default]
    All,
    /// Apply this number of pending migrations
    Steps { num: usize },
    /// Up to migration name
    To { name: String },
}

impl MigrationManager {
    pub async fn from_config(config: &crate::config::Config) -> Result<Self> {
        let table: Iden = config.migrations.entity().clone();

        Ok(Self {
            out_dir: config.out_dir(),
            table: table.clone(),
            prefix: config.migrations.prefix(),
            engine: Engine::from_config(config).await?,
        })
    }

    pub fn new(out_dir: impl Into<PathBuf>, engine: Engine) -> Self {
        Self {
            out_dir: out_dir.into(),
            table: engine.table().clone(),
            prefix: MigrationPrefix::Index,
            engine,
        }
    }

    pub fn with_out_dir(mut self, out_dir: impl Into<PathBuf>) -> Self {
        self.out_dir = out_dir.into();
        self
    }

    /// Set the migration table name
    pub fn with_table_name(mut self, name: impl Into<String>) -> Self {
        self.table = (name.into(), self.table.schema.clone()).into();
        self.engine = self.engine.with_table(self.table.clone());
        self
    }

    /// Set the migration table schema
    pub fn with_table_schema(mut self, schema: impl Into<String>) -> Self {
        self.table = (self.table.name.clone(), Some(schema.into())).into();
        self.engine = self.engine.with_table(self.table.clone());
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
        std::fs::create_dir_all(self.meta_dir())?;
        Ok(())
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.out_dir.join("_meta")
    }

    pub fn journal_path(&self) -> PathBuf {
        journal_path(&self.out_dir)
    }

    pub fn load_journal(&self) -> Result<Journal> {
        Journal::load(&self.journal_path())
    }

    pub fn save_journal(&self, journal: &Journal) -> Result<()> {
        journal.save(&self.journal_path())
    }

    pub fn record_migration_in_journal(
        &self,
        migration_path: &Path,
        kind: MigrationKind,
    ) -> Result<()> {
        let mut journal = self.load_journal()?;
        journal.record_migration(migration_path, kind)?;
        self.save_journal(&journal)
    }

    /// Generate the next migration name
    pub fn next_migration_name(&self, suffix: Option<impl ToString>) -> Result<String> {
        let existing = self.list_up_migrations()?;

        let suffix = match suffix {
            Some(s) => s.to_string(),
            None => petname::Petnames::default()
                .namer(2, "-")
                .iter(&mut rand::rng())
                .next()
                .expect("no names available")
                .to_lowercase(),
        };

        match self.prefix {
            MigrationPrefix::Index => {
                let next_idx = existing
                    .iter()
                    .filter_map(|path| {
                        path.file_stem()
                            .and_then(|stem| stem.to_str())
                            .and_then(|stem| stem.split('_').next())
                            .and_then(|prefix| prefix.parse::<usize>().ok())
                    })
                    .max()
                    .map(|idx| idx + 1)
                    .unwrap_or(0);
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

    /// Get down migration path
    pub fn get_up_migration_path(&self, up_migration_name: &str) -> PathBuf {
        self.out_dir.join(format!("{}.sql", up_migration_name))
    }

    /// Get the down migration path for a given up migration
    pub fn get_down_migration_path(&self, up_migration_name: &str) -> PathBuf {
        self.out_dir.join(format!("{}.down.sql", up_migration_name))
    }

    /// Check if a down migration exists for a given up migration
    pub fn has_down_migration(&self, up_migration_name: &str) -> bool {
        self.get_down_migration_path(up_migration_name).exists()
    }

    /// Create the migrations table if it doesn't exist
    pub async fn ensure_migrations_table(&self) -> Result<()> {
        self.engine.ensure_migrations().await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to create migrations table '{}': {}",
                self.table.name, e
            ))
        })?;
        Ok(())
    }

    /// Get list of applied migrations from the database
    pub async fn get_applied_migrations(&self) -> Result<Vec<MigrationRow>> {
        self.ensure_migrations_table().await?;
        let rows = self.engine.select_migrations().await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to query applied migrations from table '{}': {}",
                self.table.name, e
            ))
        })?;
        Ok(rows)
    }

    /// Get applied migrations without creating the migrations table.
    pub async fn try_get_applied_migrations(&self) -> Result<Vec<MigrationRow>> {
        if !self.engine.migrations_table_exists().await? {
            return Ok(Vec::new());
        }

        self.engine.select_migrations().await.map_err(|e| {
            ShkiError::migration(format!(
                "Failed to query applied migrations from table '{}': {}",
                self.table.name, e
            ))
        })
    }

    /// Get pending migrations
    pub async fn get_pending_migrations(&self) -> Result<Vec<PathBuf>> {
        let all_migrations = self.list_up_migrations()?;
        let applied = self.get_applied_migrations().await?;

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
    pub async fn validate_checksums(&self) -> Result<()> {
        let applied = self.get_applied_migrations().await?;
        self.validate_applied_checksums(applied)
    }

    pub async fn validate_existing_checksums(&self) -> Result<()> {
        if !self.engine.migrations_table_exists().await? {
            return Ok(());
        }
        let applied = self.engine.select_migrations().await?;
        self.validate_applied_checksums(applied)
    }

    fn validate_applied_checksums(&self, applied: Vec<MigrationRow>) -> Result<()> {
        for migration in applied {
            let Some(stored_checksum) = migration.checksum else {
                continue;
            };

            let migration_path = self.out_dir.join(format!("{}.sql", migration.name));
            if !migration_path.exists() {
                continue;
            }

            let sql = std::fs::read_to_string(&migration_path)?;
            let current_checksum = sql_checksum(&sql);

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
    pub async fn apply_migration(&self, migration_path: &Path) -> Result<MigrationRow> {
        self.engine.apply_migration(migration_path).await
    }

    /// Record an existing migration file as applied without executing its SQL.
    ///
    /// This is useful for bootstrap/adoption workflows where the database already
    /// matches the migration state and only tracking metadata should be inserted.
    pub async fn mark_migration_applied(&self, migration_path: &Path) -> Result<()> {
        self.engine.mark_applied(migration_path).await?;
        Ok(())
    }

    pub async fn apply(&self, mode: ApplyMigrationMode) -> Result<Vec<String>> {
        self.validate_checksums().await?;

        let pending = self.get_pending_migrations().await?;
        let mut applied = Vec::new();

        let num = match mode {
            ApplyMigrationMode::All => Ok(pending.len()),
            ApplyMigrationMode::Steps { num } => Ok(num),
            ApplyMigrationMode::To { name } => {
                let path = pending.iter().enumerate().find(|(_, p)| {
                    p.file_stem()
                        .is_some_and(|p| p.to_str().unwrap() == name.as_str())
                });
                match path {
                    Some((idx, _)) => Ok(idx + 1),
                    None => Err(ShkiError::migration(format!(
                        "Migration target '{}' is not pending",
                        name
                    ))),
                }
            }
        }?;

        for migration_path in pending.iter().take(num) {
            let name = migration_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| ShkiError::migration("Invalid migration filename"))?
                .to_string();

            self.apply_migration(migration_path).await?;
            applied.push(name);
        }
        Ok(applied)
    }

    /// Get migrations that can be rolled back (applied migrations that have a down file)
    ///
    /// Returns migrations in reverse order (most recent first) that:
    /// 1. Have been applied to the database
    /// 2. Have a corresponding .down.sql file
    pub async fn get_rollback_migrations(&self) -> Result<Vec<PathBuf>> {
        let applied = self.get_applied_migrations().await?;

        let rollback_migrations: Vec<PathBuf> = applied
            .into_iter()
            .rev()
            .filter_map(|m| {
                let down_path = self.out_dir.join(format!("{}.down.sql", m.name));
                if down_path.exists() {
                    Some(down_path)
                } else {
                    None
                }
            })
            .collect();

        Ok(rollback_migrations)
    }

    /// Rollback a single migration using its down migration file
    ///
    /// Executes the down migration within a transaction and removes
    /// the migration record from the migrations table.
    pub async fn rollback_migration(&self, down_migration_path: &Path) -> Result<()> {
        self.engine.rollback_migration(down_migration_path).await
    }

    /// Rollback migrations until there are no more down migrations available
    ///
    /// This will rollback applied migrations in reverse order, stopping when:
    /// - All applied migrations with down files have been rolled back
    /// - An error occurs
    ///
    /// Returns the names of migrations that were rolled back.
    pub async fn rollback_all(&self) -> Result<Vec<String>> {
        let rollback_migrations = self.get_rollback_migrations().await?;
        let mut rolled_back = Vec::new();

        for down_path in rollback_migrations {
            let name = down_path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".down.sql"))
                .ok_or_else(|| ShkiError::migration("Invalid down migration filename"))?
                .to_string();

            self.rollback_migration(&down_path).await?;
            rolled_back.push(name);
        }

        Ok(rolled_back)
    }

    /// Rollback a specific number of migrations
    ///
    /// Rolls back up to `count` migrations in reverse order.
    /// Only migrations with down files can be rolled back.
    pub async fn rollback_count(&self, count: usize) -> Result<Vec<String>> {
        let rollback_migrations = self.get_rollback_migrations().await?;
        let mut rolled_back = Vec::new();

        for down_path in rollback_migrations.into_iter().take(count) {
            let name = down_path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".down.sql"))
                .ok_or_else(|| ShkiError::migration("Invalid down migration filename"))?
                .to_string();

            self.rollback_migration(&down_path).await?;
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

        let sanitized_name = sanitize_migration_name(name);
        let migration_name = self.next_migration_name(Some(&sanitized_name))?;
        let up_path = self.out_dir.join(format!("{}.sql", migration_name));

        let up_content = generate_blank_migration_template(&migration_name, false);
        std::fs::write(&up_path, up_content)?;

        let down_path = if with_down {
            let path = self.out_dir.join(format!("{}.down.sql", migration_name));
            let down_content = generate_blank_migration_template(&migration_name, true);
            std::fs::write(&path, down_content)?;
            Some(path)
        } else {
            None
        };

        self.record_migration_in_journal(&up_path, MigrationKind::Custom)?;

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

        let up_content = self.migration_content(&migration_name, up_sql, MigrationDirection::Up);

        std::fs::write(&up_path, up_content)?;

        let down_path = if let Some(down) = down_sql {
            let path = self.out_dir.join(format!("{}.down.sql", migration_name));
            let down_content =
                self.migration_content(&migration_name, Some(down), MigrationDirection::Down);

            std::fs::write(&path, down_content)?;
            Some(path)
        } else {
            None
        };

        self.record_migration_in_journal(&up_path, MigrationKind::Custom)?;

        Ok((up_path, down_path))
    }

    /// Remove all snapshots linked to a migration name.
    ///
    /// Returns the number of snapshot files removed.
    pub fn remove_snapshots_for_migration(&self, migration_name: &str) -> Result<usize> {
        let meta_dir = self.out_dir.join("_meta");
        if !meta_dir.exists() {
            return Ok(0);
        }

        let mut removed = 0usize;

        let derived_path = meta_dir.join(format!("{}.snapshot.json", migration_name));
        if derived_path.exists() {
            std::fs::remove_file(&derived_path)?;
            removed += 1;
        }

        for entry in std::fs::read_dir(&meta_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path == derived_path {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("_journal.json") {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let parsed = Snapshot::from_json(&path)?;

            let is_match = parsed
                .migration
                .as_ref()
                .map(|m| m.name == migration_name)
                .unwrap_or(false);

            if is_match {
                std::fs::remove_file(path)?;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SqlDialect;
    use tempfile::TempDir;

    fn temp_manager() -> (TempDir, MigrationManager) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let manager = MigrationManager::new(
            temp_dir.path(),
            Engine::detached(SqlDialect::Sqlite, Iden::new("__shki_migrations", None)),
        );
        (temp_dir, manager)
    }

    fn sqlite_manager() -> (TempDir, MigrationManager) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        std::fs::File::create(&db_path).expect("failed to create sqlite db file");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy(&format!("sqlite://{}", db_path.display()))
            .expect("failed to create sqlite pool");
        let manager = MigrationManager::new(
            temp_dir.path(),
            Engine::Sqlite(crate::engines::sqlite::Sqlite::new(
                pool,
                Iden::new("__shki_migrations", None),
            )),
        );
        (temp_dir, manager)
    }

    fn write_sqlite_migrations(manager: &MigrationManager, names: &[&str]) {
        for (idx, name) in names.iter().enumerate() {
            std::fs::write(
                manager.out_dir.join(format!("{name}.sql")),
                format!("CREATE TABLE migration_{idx} (id INTEGER PRIMARY KEY);"),
            )
            .expect("failed to write migration");
        }
    }

    async fn applied_names(manager: &MigrationManager) -> Vec<String> {
        manager
            .get_applied_migrations()
            .await
            .expect("failed to load applied migrations")
            .into_iter()
            .map(|migration| migration.name)
            .collect()
    }

    #[test]
    fn list_migration_files_splits_up_and_down_entries() {
        let (_temp_dir, manager) = temp_manager();
        std::fs::write(manager.out_dir.join("0002_second.sql"), "SELECT 2;")
            .expect("failed to write up migration");
        std::fs::write(manager.out_dir.join("0001_first.sql"), "SELECT 1;")
            .expect("failed to write up migration");
        std::fs::write(manager.out_dir.join("0002_second.down.sql"), "SELECT 2;")
            .expect("failed to write down migration");

        let up = manager
            .list_up_migrations()
            .expect("failed to list up migrations");
        let down = manager
            .list_down_migrations()
            .expect("failed to list down migrations");

        assert_eq!(
            up.iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["0001_first.sql", "0002_second.sql"]
        );
        assert_eq!(
            down.iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["0002_second.down.sql"]
        );
    }

    #[test]
    fn next_index_uses_highest_existing_prefix() {
        let (_temp_dir, manager) = temp_manager();
        std::fs::write(manager.out_dir.join("0000_initial.sql"), "SELECT 0;")
            .expect("failed to write migration");
        std::fs::write(manager.out_dir.join("0003_existing.sql"), "SELECT 3;")
            .expect("failed to write migration");

        let name = manager
            .next_migration_name(Some("add-users"))
            .expect("failed to generate migration name");

        assert_eq!(name, "0004_add-users");
    }

    #[test]
    fn create_blank_migration_with_content_writes_expected_files() {
        let (_temp_dir, manager) = temp_manager();
        let (up_path, down_path) = manager
            .create_blank_migration_with_content_and_down(
                "Add users table",
                Some("CREATE TABLE users (id INTEGER PRIMARY KEY);"),
                Some("DROP TABLE users;"),
            )
            .expect("failed to create migration files");

        let up = std::fs::read_to_string(&up_path).expect("failed to read up migration");
        let down = std::fs::read_to_string(
            down_path
                .as_ref()
                .expect("down migration should be created"),
        )
        .expect("failed to read down migration");

        assert!(up_path.ends_with("0000_add-users-table.sql"));
        assert!(up.contains("-- Migration: 0000_add-users-table (up)"));
        assert!(up.contains("CREATE TABLE users (id INTEGER PRIMARY KEY);\n"));
        assert!(down.contains("-- Migration: 0000_add-users-table (down)"));
        assert!(down.contains("DROP TABLE users;\n"));

        let journal = manager.load_journal().expect("journal should load");
        assert_eq!(journal.entries.len(), 1);
        let entry = &journal.entries[0];
        assert_eq!(entry.migration, "0000_add-users-table");
        assert_eq!(entry.kind, MigrationKind::Custom);
        assert_eq!(entry.checksum, sql_checksum(&up));
    }

    #[test]
    fn journal_recording_upserts_existing_migration_entry() {
        let (_temp_dir, manager) = temp_manager();
        let up_path = manager
            .create_blank_migration_with_content("custom", Some("SELECT 1;"))
            .expect("failed to create migration");

        manager
            .record_migration_in_journal(&up_path, MigrationKind::Schema)
            .expect("failed to record schema entry");

        let journal = manager.load_journal().expect("journal should load");
        assert_eq!(journal.entries.len(), 1);
        let entry = &journal.entries[0];
        assert_eq!(entry.migration, "0000_custom");
        assert_eq!(entry.kind, MigrationKind::Schema);
    }

    #[tokio::test]
    async fn validate_checksums_skips_missing_files_and_detects_mismatches() {
        let (_temp_dir, manager) = sqlite_manager();

        let missing_path = manager.out_dir.join("0000_missing.sql");
        std::fs::write(
            &missing_path,
            "CREATE TABLE missing_example (id INTEGER PRIMARY KEY);",
        )
        .expect("failed to write migration");
        manager
            .mark_migration_applied(&missing_path)
            .await
            .expect("failed to mark migration applied");
        std::fs::remove_file(&missing_path).expect("failed to remove migration file");

        let mismatch_path = manager.out_dir.join("0001_changed.sql");
        std::fs::write(
            &mismatch_path,
            "CREATE TABLE changed_example (id INTEGER PRIMARY KEY);",
        )
        .expect("failed to write migration");
        manager
            .apply_migration(&mismatch_path)
            .await
            .expect("failed to apply migration");
        std::fs::write(
            &mismatch_path,
            "CREATE TABLE changed_example (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
        )
        .expect("failed to rewrite migration");

        let error = manager
            .validate_checksums()
            .await
            .expect_err("checksum validation should fail");

        let message = error.to_string();
        assert!(message.contains("0001_changed"));
        assert!(message.contains("checksum mismatch"));
    }

    #[tokio::test]
    async fn apply_all_applies_every_pending_migration() {
        let (_temp_dir, manager) = sqlite_manager();
        write_sqlite_migrations(&manager, &["0000_first", "0001_second", "0002_third"]);

        let applied = manager
            .apply(ApplyMigrationMode::All)
            .await
            .expect("failed to apply all migrations");

        assert_eq!(applied, vec!["0000_first", "0001_second", "0002_third"]);
        assert_eq!(applied_names(&manager).await, applied);
    }

    #[tokio::test]
    async fn apply_steps_limits_pending_migrations() {
        let (_temp_dir, manager) = sqlite_manager();
        write_sqlite_migrations(&manager, &["0000_first", "0001_second", "0002_third"]);

        let applied = manager
            .apply(ApplyMigrationMode::Steps { num: 2 })
            .await
            .expect("failed to apply limited migrations");

        assert_eq!(applied, vec!["0000_first", "0001_second"]);
        assert_eq!(applied_names(&manager).await, applied);
    }

    #[tokio::test]
    async fn apply_to_applies_through_named_pending_migration() {
        let (_temp_dir, manager) = sqlite_manager();
        write_sqlite_migrations(&manager, &["0000_first", "0001_second", "0002_third"]);

        let applied = manager
            .apply(ApplyMigrationMode::To {
                name: "0001_second".to_string(),
            })
            .await
            .expect("failed to apply to named migration");

        assert_eq!(applied, vec!["0000_first", "0001_second"]);
        assert_eq!(applied_names(&manager).await, applied);
    }

    #[tokio::test]
    async fn apply_to_unknown_migration_does_not_apply_everything() {
        let (_temp_dir, manager) = sqlite_manager();
        write_sqlite_migrations(&manager, &["0000_first", "0001_second", "0002_third"]);

        let error = manager
            .apply(ApplyMigrationMode::To {
                name: "9999_missing".to_string(),
            })
            .await
            .expect_err("unknown target should fail");

        assert!(error.to_string().contains("9999_missing"));
        assert!(applied_names(&manager).await.is_empty());
    }

    #[tokio::test]
    async fn rollback_candidates_follow_applied_order_and_require_down_files() {
        let (_temp_dir, manager) = sqlite_manager();

        for name in ["0000_first", "0001_second", "0002_third"] {
            let path = manager.out_dir.join(format!("{name}.sql"));
            std::fs::write(
                &path,
                format!("CREATE TABLE {name} (id INTEGER PRIMARY KEY);"),
            )
            .expect("failed to write migration");
            manager
                .mark_migration_applied(&path)
                .await
                .expect("failed to mark migration applied");
        }

        std::fs::write(
            manager.out_dir.join("0000_first.down.sql"),
            "DROP TABLE 0000_first;",
        )
        .expect("failed to write down migration");
        std::fs::write(
            manager.out_dir.join("0002_third.down.sql"),
            "DROP TABLE 0002_third;",
        )
        .expect("failed to write down migration");

        let rollback = manager
            .get_rollback_migrations()
            .await
            .expect("failed to get rollback migrations");

        assert_eq!(
            rollback
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["0002_third.down.sql", "0000_first.down.sql"]
        );
    }
}
