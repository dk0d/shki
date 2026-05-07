use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::unique_suffix;

pub struct SqliteTestContext {
    pub temp_dir: TempDir,
    pub db_path: PathBuf,
    pub migrations_dir: PathBuf,
    pub suffix: String,
}

impl SqliteTestContext {
    pub fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join(format!("{name}.db"));
        let migrations_dir = temp_dir.path().join("migrations");

        std::fs::File::create(&db_path).expect("failed to create sqlite db file");
        std::fs::create_dir_all(&migrations_dir).expect("failed to create migrations dir");

        Self {
            temp_dir,
            db_path,
            migrations_dir,
            suffix: unique_suffix(),
        }
    }

    pub fn url(&self) -> String {
        format!("sqlite://{}", self.db_path.display())
    }

    pub async fn pool(&self) -> AnyPool {
        sqlx::any::install_default_drivers();
        AnyPoolOptions::new()
            .max_connections(1)
            .connect(&self.url())
            .await
            .expect("failed to connect to sqlite test db")
    }
}
