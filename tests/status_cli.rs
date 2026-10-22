//! Issue #3: exercise status output and exit codes through the real CLI.

use std::path::PathBuf;
use std::process::Command;

use regex::Regex;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;

struct StatusContext {
    root: TempDir,
    pool: SqlitePool,
}

struct CliOutput {
    success: bool,
    text: String,
}

impl StatusContext {
    async fn new() -> Self {
        let root = tempfile::tempdir().expect("create status fixture");
        let database = root.path().join("app.db");
        std::fs::File::create(&database).expect("create sqlite file");
        std::fs::create_dir(root.path().join("migrations")).expect("create migrations directory");
        // Stop dotenv from loading configuration from an ancestor directory.
        std::fs::write(root.path().join(".env"), "").expect("create isolated dotenv file");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}", database.display()))
            .await
            .expect("connect to fixture database");
        let ctx = Self { root, pool };
        ctx.write_config(true);
        ctx
    }

    fn write_config(&self, online: bool) {
        let mut config = toml::toml! {
            dialect = "sqlite"
            out = "migrations"
        };
        if online {
            config.insert(
                "database_url".into(),
                toml::Value::String(format!(
                    "sqlite://{}",
                    self.root.path().join("app.db").display()
                )),
            );
        }
        std::fs::write(
            self.root.path().join("shki.toml"),
            toml::to_string(&config).expect("serialize fixture config"),
        )
        .expect("write fixture config");
    }

    fn migration(&self, name: &str) -> PathBuf {
        self.root
            .path()
            .join("migrations")
            .join(format!("{name}.sql"))
    }

    fn write_migration(&self, name: &str) {
        std::fs::write(self.migration(name), "SELECT 1;\n").expect("write migration");
    }

    fn cli(&self, command: &str) -> CliOutput {
        let output = Command::new(env!("CARGO_BIN_EXE_shki"))
            .current_dir(self.root.path())
            .env_clear()
            .env("NO_COLOR", "1")
            .args(["--config", "shki.toml", "--no-color", command])
            .output()
            .expect("run shki CLI");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = Regex::new(r"\x1b\[[0-9;]*m")
            .unwrap()
            .replace_all(&text, "")
            .into_owned();
        CliOutput {
            success: output.status.success(),
            text,
        }
    }

    fn apply(&self, name: &str) {
        self.write_migration(name);
        let output = self.cli("migrate");
        assert!(output.success, "fixture migration failed:\n{}", output.text);
    }

    async fn history(&self) -> Vec<(i64, String, Option<String>, String)> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = '__shki_migrations')",
        )
        .fetch_one(&self.pool)
        .await
        .expect("check history table");
        if !exists {
            return Vec::new();
        }
        sqlx::query_as("SELECT id, name, checksum, applied_at FROM __shki_migrations ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .expect("read history")
    }

    async fn schema(&self) -> Vec<(String, Option<String>)> {
        sqlx::query_as("SELECT name, sql FROM sqlite_schema ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .expect("read schema")
    }

    async fn status(&self) -> CliOutput {
        let schema = self.schema().await;
        let history = self.history().await;
        let output = self.cli("status");
        assert_eq!(self.schema().await, schema, "status must not change schema");
        assert_eq!(
            self.history().await,
            history,
            "status must not change history"
        );
        output
    }

    async fn make_legacy(&self) {
        sqlx::query("UPDATE __shki_migrations SET checksum = NULL")
            .execute(&self.pool)
            .await
            .expect("simulate history predating checksum tracking");
    }
}

impl CliOutput {
    // Search both tables; padding, colors, and the warning heading aren't part
    // of the contract. Each migration appears once with its status label.
    fn row(&self, name: &str) -> Vec<&str> {
        let rows: Vec<Vec<&str>> = self
            .text
            .lines()
            .map(|line| line.split('|').map(str::trim).collect::<Vec<_>>())
            .filter(|cells| cells.contains(&name))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "expected exactly one table row for {name}:\n{}",
            self.text
        );
        rows.into_iter().next().unwrap()
    }

    fn row_with_status(&self, name: &str, status: &str) -> Vec<&str> {
        let row = self.row(name);
        assert_eq!(
            row[0].split_whitespace().last(),
            Some(status),
            "expected {name} to be {status}:\n{}",
            self.text
        );
        row
    }

    fn assert_missing_local(&self, name: &str) {
        self.row_with_status(name, "missing");
        assert!(
            !self.success,
            "missing applied SQL must fail status:\n{}",
            self.text
        );
    }
}

#[tokio::test]
async fn migrate_succeeds_after_applying_pending_work_with_missing_local_history() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_missing");
    std::fs::remove_file(ctx.migration("0001_missing")).unwrap();
    std::fs::write(
        ctx.migration("0002_pending"),
        "CREATE TABLE migrated (id INTEGER PRIMARY KEY);\n",
    )
    .unwrap();

    let output = ctx.cli("migrate");

    assert!(
        output.success,
        "migration must not fail during display:\n{}",
        output.text
    );
    output.row_with_status("0001_missing", "missing");
    output.row_with_status("0002_pending", "applied");
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'migrated')")
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert!(exists, "pending SQL must execute");
    assert_eq!(
        ctx.history()
            .await
            .iter()
            .map(|row| row.1.as_str())
            .collect::<Vec<_>>(),
        ["0001_missing", "0002_pending"]
    );
}

#[tokio::test]
async fn status_shows_applied_and_pending_once_with_distinct_states() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_applied");
    ctx.write_migration("0002_pending");
    let history = ctx.history().await;

    let output = ctx.status().await;

    assert!(output.success, "{}", output.text);
    let applied = output.row_with_status("0001_applied", "applied");
    output.row_with_status("0002_pending", "pending");
    assert!(
        applied.contains(&history[0].2.as_deref().unwrap()),
        "{}",
        output.text
    );
    assert!(applied.contains(&history[0].3.as_str()), "{}", output.text);
    assert_eq!(
        std::fs::read_to_string(ctx.migration("0002_pending")).unwrap(),
        "SELECT 1;\n"
    );
}

#[tokio::test]
async fn status_with_pending_only_succeeds_without_creating_history() {
    let ctx = StatusContext::new().await;
    ctx.write_migration("0001_pending");

    let output = ctx.status().await;

    assert!(output.success, "{}", output.text);
    output.row_with_status("0001_pending", "pending");
    assert!(ctx.schema().await.is_empty());
}

#[tokio::test]
async fn status_with_no_local_or_applied_migrations_succeeds_without_creating_history() {
    let ctx = StatusContext::new().await;

    let output = ctx.status().await;

    assert!(output.success, "{}", output.text);
    assert!(
        output.text.to_lowercase().contains("no migrations"),
        "{}",
        output.text
    );
    assert!(ctx.schema().await.is_empty());
}

#[tokio::test]
async fn status_reports_db_only_migration_alongside_applied_and_pending() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_missing");
    ctx.apply("0002_applied");
    ctx.write_migration("0003_pending");
    let history = ctx.history().await;
    std::fs::remove_file(ctx.migration("0001_missing")).unwrap();

    let output = ctx.status().await;

    let missing = output.row_with_status("0001_missing", "missing");
    assert!(
        missing.contains(&history[0].2.as_deref().unwrap()),
        "{}",
        output.text
    );
    assert!(missing.contains(&history[0].3.as_str()), "{}", output.text);
    output.row_with_status("0002_applied", "applied");
    output.row_with_status("0003_pending", "pending");
    output.assert_missing_local("0001_missing");
}

#[tokio::test]
async fn status_reports_db_history_when_local_directory_is_empty() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_applied");
    std::fs::remove_file(ctx.migration("0001_applied")).unwrap();

    let output = ctx.status().await;

    assert!(
        !output.text.to_lowercase().contains("no migrations found"),
        "{}",
        output.text
    );
    output.assert_missing_local("0001_applied");
}

#[tokio::test]
async fn status_reports_db_history_when_local_directory_is_absent() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_applied");
    let directory = ctx.root.path().join("migrations");
    std::fs::remove_dir_all(&directory).unwrap();

    let output = ctx.status().await;

    assert!(
        !directory.exists(),
        "status must not recreate the migration directory"
    );
    output.assert_missing_local("0001_applied");
}

#[tokio::test]
async fn status_checksum_mismatch_prints_migration_and_exits_unsuccessfully() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_changed");
    std::fs::write(ctx.migration("0001_changed"), "SELECT 2;\n").unwrap();

    let output = ctx.status().await;

    let text = output.text.to_lowercase();
    assert!(
        text.contains("checksum") && text.contains("mismatch"),
        "{}",
        output.text
    );
    assert!(
        !output.success,
        "checksum mismatch must fail status:\n{}",
        output.text
    );
    output.row("0001_changed");
    assert_eq!(
        std::fs::read_to_string(ctx.migration("0001_changed")).unwrap(),
        "SELECT 2;\n"
    );
}

#[tokio::test]
async fn status_legacy_history_without_checksum_accepts_existing_local_file() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_legacy");
    ctx.make_legacy().await;
    std::fs::write(ctx.migration("0001_legacy"), "SELECT 2;\n").unwrap();

    let output = ctx.status().await;

    assert!(output.success, "{}", output.text);
    output.row_with_status("0001_legacy", "applied");
    assert!(
        !output.text.to_lowercase().contains("mismatch"),
        "{}",
        output.text
    );
}

#[tokio::test]
async fn status_legacy_history_without_checksum_still_rejects_missing_local_file() {
    let ctx = StatusContext::new().await;
    ctx.apply("0001_legacy");
    ctx.make_legacy().await;
    std::fs::remove_file(ctx.migration("0001_legacy")).unwrap();

    let output = ctx.status().await;

    output.assert_missing_local("0001_legacy");
}

#[tokio::test]
async fn status_without_database_url_shows_local_files_without_claiming_applied() {
    let ctx = StatusContext::new().await;
    ctx.write_config(false);
    ctx.write_migration("0001_local");

    let output = ctx.status().await;

    assert!(output.success, "{}", output.text);
    output.row_with_status("0001_local", "pending");
    assert!(
        Regex::new(r"(?i)no database|offline|local.only|not connected")
            .unwrap()
            .is_match(&output.text),
        "offline status must make the lack of database validation clear:\n{}",
        output.text
    );
    assert!(ctx.schema().await.is_empty());
}
