use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, migrate::checksum::sql_checksum};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub version: String,

    #[serde(default)]
    pub entries: Vec<JournalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub migration: String,
    pub kind: MigrationKind,
    pub checksum: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationKind {
    Schema,
    Custom,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            entries: Vec::new(),
        }
    }
}

impl Journal {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn upsert_entry(&mut self, entry: JournalEntry) {
        self.entries
            .retain(|existing| existing.migration != entry.migration);
        self.entries.push(entry);
    }

    pub fn record_migration(&mut self, migration_path: &Path, kind: MigrationKind) -> Result<()> {
        let migration = migration_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_string();
        let sql = std::fs::read_to_string(migration_path)?;

        self.upsert_entry(JournalEntry {
            migration,
            kind,
            checksum: sql_checksum(&sql),
        });

        Ok(())
    }
}

pub fn journal_path(out_dir: &Path) -> PathBuf {
    out_dir.join("_meta").join("_journal.json")
}
