use std::path::Path;

use postgresql_embedded::{Settings, Version};

use crate::{Result, ShkiError};

const RELEASES: &str = "https://github.com/theseus-rs/postgresql-binaries/releases";

/// Install embedded PostgreSQL from a release archive the user downloaded by
/// hand, so machines that cannot reach the GitHub API (rate-limited or
/// filtering proxies, air-gapped hosts) can still compile Declarative Schemas.
/// Extracts into the same directory the automatic download uses, so later
/// compiles find it and make no network request.
pub async fn cmd_shadow_install(archive: &Path) -> Result<()> {
    let file_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let version = archive_version(file_name)?;
    let target = postgresql_archive::matcher::target();
    if !file_name.contains(target) {
        return Err(ShkiError::schema(format!(
            "{file_name} is not built for this machine ({target}). Download postgresql-{version}-{target}.tar.gz from {RELEASES} instead."
        )));
    }

    let settings = Settings::default();
    let out_dir = settings.installation_dir.join(version.to_string());
    if out_dir.exists() {
        println!(
            "PostgreSQL {version} is already installed at {}",
            out_dir.display()
        );
        return Ok(());
    }

    let bytes = std::fs::read(archive)?;
    postgresql_archive::extract(&settings.releases_url, &bytes, &out_dir)
        .await
        .map_err(|err| ShkiError::schema(format!("Failed to extract {file_name}: {err}")))?;
    println!(
        "Installed PostgreSQL {version} to {}. Compile with --pg-version {} to use it.",
        out_dir.display(),
        version.major
    );
    Ok(())
}

/// `postgresql-18.4.0-aarch64-apple-darwin.tar.gz` -> `18.4.0`
fn archive_version(file_name: &str) -> Result<Version> {
    file_name
        .strip_prefix("postgresql-")
        .and_then(|rest| rest.split('-').next())
        .and_then(|version| Version::parse(version).ok())
        .ok_or_else(|| {
            ShkiError::schema(format!(
                "Expected a release archive named postgresql-<version>-<target>.tar.gz from {RELEASES}, got {file_name}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_from_release_asset_name() {
        assert_eq!(
            archive_version("postgresql-18.4.0-aarch64-apple-darwin.tar.gz").unwrap(),
            Version::new(18, 4, 0)
        );
        assert!(archive_version("pg.tar.gz").is_err());
        assert!(archive_version("postgresql-latest-x86_64.tar.gz").is_err());
    }
}
