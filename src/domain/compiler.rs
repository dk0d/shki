use std::future::Future;

use async_trait::async_trait;
use postgresql_embedded::{
    PostgreSQL as EmbeddedPostgres, Settings, SettingsBuilder, Status, VersionReq,
};
use sqlx::{AssertSqlSafe, Executor};
use uuid::Uuid;

use crate::config::Config;
use crate::declarative::{load_declarative_schema, normalize_declarative_apply_sql};
use crate::diff::{diff_snapshots, load_latest_snapshot, load_snapshot_by_name};
use crate::engines::Engine;
use crate::engines::pg::Postgres;
use crate::migrate::checksum::sql_checksum;
use crate::migrate::manager::{MigrationInfo, MigrationManager};
use crate::schema::SqlDialect;
use crate::snapshots::{Introspectable, Snapshot};
use crate::sql::render::SqlRenderer;

use crate::{Result, ShkiError};

const MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS: u64 = 120;
const EXTERNAL_SHADOW_OWNER_MARKER: &str = "shki:shadow";

#[async_trait]
pub trait SchemaCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot>;
    async fn validate_generated_diff_sql(
        &self,
        config: &Config,
        baseline: &Snapshot,
        generated_sql: &str,
    ) -> Result<()>;
}

pub fn compiler_from_config(config: &Config) -> Result<Box<dyn SchemaCompiler + Send + Sync>> {
    if config.shadow.shadow_database_url.is_some() {
        Ok(Box::new(ExternalShadowDBCompiler::from_config(config)?))
    } else {
        Ok(Box::new(EmbeddedShadowDBCompiler::from_config(config)?))
    }
}

async fn with_embedded_shadow_pool<T, F, Fut>(config: &Config, operation: F) -> Result<T>
where
    F: FnOnce(sqlx::Pool<sqlx::Postgres>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut server = EmbeddedPostgres::new(embedded_shadow_settings(config)?);
    if is_embedded_not_installed(server.status()) {
        println!("{}", first_start_message(shadow_timeout_seconds(config)));
    }
    server
        .setup()
        .await
        .map_err(|err| ShkiError::schema(server_setup_failed_message(err)))?;
    server
        .start()
        .await
        .map_err(|err| ShkiError::schema(server_start_failed_message(err)))?;

    let database_name = format!("shki_shadow_{}", Uuid::new_v4().simple());

    server
        .create_database(&database_name)
        .await
        .map_err(|err| {
            ShkiError::schema(format!(
                "Failed to create embedded Shadow Database: {}",
                err
            ))
        })?;
    let database_url = server.settings().url(&database_name);
    let pool = connect_postgres(config, &database_url).await?;
    let result = operation(pool).await;
    let stop_result = server.stop().await.map_err(|err| {
        ShkiError::schema(format!("Failed to stop embedded Shadow Database: {}", err))
    });

    match (result, stop_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn embedded_shadow_settings(config: &Config) -> Result<Settings> {
    let mut settings = SettingsBuilder::new().timeout(Some(std::time::Duration::from_secs(
        shadow_timeout_seconds(config),
    )));
    settings = settings.version(postgres_major_version_req(config.shadow.pg_version.unwrap_or(18))?);
    Ok(settings.build())
}

fn shadow_timeout_seconds(config: &Config) -> u64 {
    config
        .timeout_seconds
        .max(MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS)
}

fn first_start_message(timeout_seconds: u64) -> String {
    format!(
        "Starting embedded postgres for schema compilation. First use may download/install PostgreSQL and initialize a local data directory; waiting up to {timeout_seconds}s."
    )
}

fn server_setup_failed_message(err: postgresql_embedded::Error) -> String {
    format!(
        r#"Failed to set up embedded Shadow Database: {}.
On first use, shki may need to download/install PostgreSQL binaries and initialize a local data directory.
Try again after checking network access, or configure shadow_database_url to use your own disposable PostgreSQL database.
"#,
        err
    )
}

fn server_start_failed_message(err: postgresql_embedded::Error) -> String {
    format!(
        r#"Failed to start embedded Shadow Database: {}.
The embedded database can take longer on first use while PostgreSQL finishes initialization.
Try again, increase timeout_seconds, or configure shadow_database_url to use your own disposable PostgreSQL database.
"#,
        err,
    )
}

fn is_embedded_not_installed(status: Status) -> bool {
    matches!(status, Status::NotInstalled)
}

fn ensure_postgres_compiler_config(config: &Config) -> Result<()> {
    if config.dialect() != SqlDialect::Postgres {
        return Err(ShkiError::unsupported_dialect(
            "Declarative Schema compilation currently requires PostgreSQL",
        ));
    }

    Ok(())
}

#[derive(Debug, Default)]
pub struct EmbeddedShadowDBCompiler;

impl EmbeddedShadowDBCompiler {
    pub fn from_config(config: &Config) -> Result<Self> {
        ensure_postgres_compiler_config(config)?;
        if let Some(version) = config.shadow.pg_version {
            postgres_major_version_req(version)?;
        }

        Ok(Self)
    }
}

#[async_trait]
impl SchemaCompiler for EmbeddedShadowDBCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot> {
        let schema = load_declarative_schema(config.schema_path())?;
        with_embedded_shadow_pool(config, |pool| async move {
            compile_with_pool(config, &schema.sql, pool).await
        })
        .await
    }

    async fn validate_generated_diff_sql(
        &self,
        config: &Config,
        baseline: &Snapshot,
        generated_sql: &str,
    ) -> Result<()> {
        if generated_diff_sql_is_empty(generated_sql) {
            return Ok(());
        }

        with_embedded_shadow_pool(config, |pool| async move {
            validate_generated_diff_sql_with_pool(config, baseline, generated_sql, pool).await
        })
        .await
    }
}

#[derive(Debug)]
pub struct ExternalShadowDBCompiler {
    shadow_database_url: String,
}

impl ExternalShadowDBCompiler {
    pub fn from_config(config: &Config) -> Result<Self> {
        ensure_postgres_compiler_config(config)?;

        let shadow_database_url = config.shadow.shadow_database_url.clone().ok_or_else(|| {
            ShkiError::config("shadow_database_url is required to compile a Declarative Schema")
        })?;

        if config.database_url() == Some(shadow_database_url.as_str()) {
            return Err(ShkiError::config(
                "shadow_database_url must not be the same as database_url",
            ));
        }

        Ok(Self {
            shadow_database_url,
        })
    }

    async fn connect(&self, config: &Config) -> Result<sqlx::Pool<sqlx::Postgres>> {
        let pool = connect_postgres(config, &self.shadow_database_url).await?;
        verify_external_shadow_ownership(&pool).await?;
        Ok(pool)
    }
}

#[async_trait]
impl SchemaCompiler for ExternalShadowDBCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot> {
        let schema = load_declarative_schema(config.schema_path())?;
        let pool = self.connect(config).await?;

        compile_with_pool(config, &schema.sql, pool).await
    }

    async fn validate_generated_diff_sql(
        &self,
        config: &Config,
        baseline: &Snapshot,
        generated_sql: &str,
    ) -> Result<()> {
        if generated_diff_sql_is_empty(generated_sql) {
            return Ok(());
        }

        let pool = self.connect(config).await?;
        validate_generated_diff_sql_with_pool(config, baseline, generated_sql, pool).await
    }
}

async fn connect_postgres(
    config: &Config,
    database_url: &str,
) -> Result<sqlx::Pool<sqlx::Postgres>> {
    Ok(sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
        .connect(database_url)
        .await?)
}

/// External shadow databases survive beyond this process. Require an explicit
/// database-level marker before the reset path can remove their schemas.
async fn verify_external_shadow_ownership(pool: &sqlx::Pool<sqlx::Postgres>) -> Result<()> {
    let marker: Option<String> = sqlx::query_scalar(
        "SELECT shobj_description(oid, 'pg_database') FROM pg_database WHERE datname = current_database()",
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        ShkiError::schema(format!(
            "Failed to verify external Shadow Database ownership: {}",
            err
        ))
    })?;

    if marker.as_deref() == Some(EXTERNAL_SHADOW_OWNER_MARKER) {
        return Ok(());
    }

    Err(ShkiError::config(format!(
        "shadow_database_url must point to a Shki-owned database. Mark the disposable database with: COMMENT ON DATABASE <shadow_database_name> IS '{}';",
        EXTERNAL_SHADOW_OWNER_MARKER
    )))
}

fn postgres_major_version_req(version: u8) -> Result<VersionReq> {
    match version {
        14..=18 => VersionReq::parse(format!("={}", version).as_str()).map_err(|err| {
            ShkiError::config(format!(
                "Failed to parse PostgreSQL major version {}: {}",
                version, err
            ))
        }),
        _ => Err(ShkiError::config(format!(
            "Unsupported embedded Shadow Database PostgreSQL version {}. Supported versions: 14, 15, 16, 17, 18",
            version
        ))),
    }
}

async fn compile_with_pool(
    config: &Config,
    schema_sql: &str,
    pool: sqlx::Pool<sqlx::Postgres>,
) -> Result<Snapshot> {
    reset_shadow_database(&pool).await?;
    apply_declarative_schema_sql(&pool, schema_sql).await?;

    let engine = Postgres::new(pool, config.migrations.entity());
    introspect_all_schemas(config, &engine).await
}

/// Compile the Declarative Schema into a shadow database, then hand the caller
/// both the resulting [`Snapshot`] and a pool connected to the freshly compiled
/// database so it can run further work against the live schema (e.g. describing
/// queries for query codegen).
///
/// This mirrors [`compiler_from_config`]: it uses the external Shadow Database
/// when `shadow_database_url` is configured, otherwise an embedded one.
pub async fn with_compiled_shadow<T, F, Fut>(config: &Config, operation: F) -> Result<T>
where
    F: FnOnce(Snapshot, sqlx::Pool<sqlx::Postgres>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    ensure_postgres_compiler_config(config)?;
    let schema = load_declarative_schema(config.schema_path())?;

    with_shadow_pool(config, |pool| async move {
        let snapshot = compile_with_pool(config, &schema.sql, pool.clone()).await?;
        operation(snapshot, pool).await
    })
    .await
}

/// Run `operation` against a connected Shadow Database pool — external when
/// `shadow_database_url` is configured, otherwise a freshly provisioned embedded
/// one. The pool's schema state is whatever the caller makes it; nothing is
/// compiled into it first.
async fn with_shadow_pool<T, F, Fut>(config: &Config, operation: F) -> Result<T>
where
    F: FnOnce(sqlx::Pool<sqlx::Postgres>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if config.shadow.shadow_database_url.is_some() {
        let compiler = ExternalShadowDBCompiler::from_config(config)?;
        let pool = compiler.connect(config).await?;
        operation(pool).await
    } else {
        with_embedded_shadow_pool(config, operation).await
    }
}

/// Resolve the baseline [`Snapshot`] to diff a new schema change against,
/// completing the Snapshot chain for any custom migrations first.
///
/// Schema-derived migrations record a Snapshot at `generate` time, but custom
/// migrations are hand-written SQL whose final form isn't known until the user
/// finishes editing — so they aren't snapshotted at creation. A custom migration
/// may still change the schema shape, which would otherwise be invisible to the
/// next diff (the baseline would be the last *schema* Snapshot, missing the
/// custom changes).
///
/// This walks the Journal, finds every trailing migration without a Snapshot,
/// replays them in order on a Shadow Database seeded from the last known
/// Snapshot, introspects after each, and persists a `<migration>.snapshot.json`
/// for it. The returned Snapshot is the resulting state — so the diff baseline
/// reflects custom-migration changes.
pub async fn resolve_baseline_snapshot(config: &Config) -> Result<Snapshot> {
    backfill_pending_snapshots(config).await?;
    load_latest_snapshot(config)
}

/// Replay and snapshot any committed migrations that don't yet have a Snapshot
/// (see [`resolve_baseline_snapshot`]). A no-op (no Shadow Database needed) when
/// every migration is already snapshotted, which is the common case.
async fn backfill_pending_snapshots(config: &Config) -> Result<()> {
    let manager = MigrationManager::new(
        config.out_dir(),
        Engine::detached(config.dialect(), config.migrations.entity()),
    );
    let journal = manager.load_journal()?;
    let meta_dir = manager.meta_dir();

    // The Journal splits into [already snapshotted… | trailing un-snapshotted].
    let last_snapshotted = journal.entries.iter().rposition(|entry| {
        meta_dir
            .join(format!("{}.snapshot.json", entry.migration))
            .exists()
    });
    let pending = match last_snapshotted {
        Some(index) => &journal.entries[index + 1..],
        None => &journal.entries[..],
    };
    if pending.is_empty() {
        return Ok(());
    }

    ensure_postgres_compiler_config(config)?;

    let base = match last_snapshotted {
        Some(index) => load_snapshot_by_name(config, &journal.entries[index].migration)?,
        None => Snapshot::new(config.dialect()),
    };

    // Read each pending migration's up SQL up front so the async block owns it.
    let pending: Vec<(String, String)> = pending
        .iter()
        .map(|entry| {
            let up_path = manager.out_dir.join(format!("{}.sql", entry.migration));
            let sql = std::fs::read_to_string(&up_path).map_err(|err| {
                ShkiError::migration(format!(
                    "Failed to read migration {} for Snapshot backfill: {}",
                    up_path.display(),
                    err
                ))
            })?;
            Ok((entry.migration.clone(), sql))
        })
        .collect::<Result<_>>()?;

    with_shadow_pool(config, |pool| async move {
        reset_shadow_database(&pool).await?;

        // Seed the last known schema shape, then replay each pending migration.
        let base_sql = render_baseline_sql(config, &base)?;
        if !base_sql.trim().is_empty() {
            sqlx::raw_sql(AssertSqlSafe(base_sql))
                .execute(&pool)
                .await
                .map_err(|err| {
                    ShkiError::schema(format!(
                        "Failed to seed baseline Snapshot before custom-migration replay: {}",
                        err
                    ))
                })?;
        }

        let mut prev_id = base.id;
        for (name, sql) in pending {
            sqlx::raw_sql(AssertSqlSafe(sql.clone()))
                .execute(&pool)
                .await
                .map_err(|err| {
                    ShkiError::database_with_source(
                        err,
                        "Migration failed to apply during Snapshot backfill in the Shadow Database",
                        &name,
                        &sql,
                    )
                })?;

            let engine = Postgres::new(pool.clone(), config.migrations.entity());
            let mut snapshot = introspect_all_schemas(config, &engine).await?;
            snapshot.prev_id = Some(prev_id.clone());
            snapshot.migration = Some(MigrationInfo {
                name: name.clone(),
                checksum: Some(sql_checksum(&sql)),
            });
            std::fs::write(meta_dir.join(format!("{}.snapshot.json", name)), snapshot.to_json()?)?;
            prev_id = snapshot.id;
        }

        Ok(())
    })
    .await
}

async fn apply_declarative_schema_sql(
    pool: &sqlx::Pool<sqlx::Postgres>,
    schema_sql: &str,
) -> Result<()> {
    let apply_sql = normalize_declarative_apply_sql(schema_sql)?;
    sqlx::raw_sql(AssertSqlSafe(apply_sql))
        .execute(pool)
        .await
        .map_err(|err| {
            ShkiError::schema(format!(
                "Failed to apply Declarative Schema to Shadow Database: {}",
                err,
            ))
        })?;

    Ok(())
}

async fn validate_generated_diff_sql_with_pool(
    config: &Config,
    baseline: &Snapshot,
    generated_sql: &str,
    pool: sqlx::Pool<sqlx::Postgres>,
) -> Result<()> {
    reset_shadow_database(&pool).await?;

    let baseline_sql = render_baseline_sql(config, baseline)?;
    if !baseline_sql.trim().is_empty() {
        sqlx::raw_sql(AssertSqlSafe(baseline_sql))
            .execute(&pool)
            .await
            .map_err(|err| {
                ShkiError::schema(format!(
                    "Failed to seed baseline Snapshot in Shadow Database before generated SQL validation: {}",
                    err
                ))
            })?;
    }

    sqlx::raw_sql(AssertSqlSafe(generated_sql))
        .execute(&pool)
        .await
        .map_err(|err| {
            ShkiError::database_with_source(
                err,
                "Generated migration SQL failed validation in Shadow Database",
                "generated migration SQL",
                generated_sql,
            )
        })?;

    Ok(())
}

fn render_baseline_sql(config: &Config, baseline: &Snapshot) -> Result<String> {
    let empty = Snapshot::new(config.dialect());
    let baseline_diff = diff_snapshots(&empty, baseline)?;
    SqlRenderer::new(&config.dialect()).generate_string(&baseline_diff.statements)
}

fn generated_diff_sql_is_empty(generated_sql: &str) -> bool {
    generated_sql.trim().is_empty()
}

async fn reset_shadow_database(pool: &sqlx::Pool<sqlx::Postgres>) -> Result<()> {
    // The configured Shadow Database is disposable. Reset all user schemas so a
    // compile starts from a clean database shape.
    pool.execute(
        r#"
        DO $$
        DECLARE
            schema_name text;
        BEGIN
            FOR schema_name IN
                SELECT nspname
                FROM pg_namespace
                WHERE nspname NOT IN ('pg_catalog', 'information_schema')
                    AND nspname NOT LIKE 'pg_toast%'
            LOOP
                EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', schema_name);
            END LOOP;
        END $$;
        CREATE SCHEMA public;
        "#,
    )
    .await
    .map_err(|err| {
        ShkiError::schema(format!(
            "Failed to reset Shadow Database before schema compilation: {}",
            err
        ))
    })?;

    Ok(())
}

async fn introspect_all_schemas(config: &Config, engine: &Postgres) -> Result<Snapshot> {
    let schema = None;
    let snapshot = engine.introspect(config, &schema).await?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_shadow_compiler_requires_postgres() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Sqlite), ..Default::default() },
            shadow: crate::ShadowArgs {
                shadow_database_url: Some("sqlite://shadow".to_string()),
                ..Default::default()
            },
            ..Config::default()
        };

        let error = ExternalShadowDBCompiler::from_config(&config)
            .expect_err("non-postgres compiler config should fail");

        assert!(error.to_string().contains("requires PostgreSQL"));
    }

    #[test]
    fn embedded_shadow_compiler_requires_postgres() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Sqlite), ..Default::default() },
            ..Config::default()
        };

        let error = EmbeddedShadowDBCompiler::from_config(&config)
            .expect_err("non-postgres compiler config should fail");

        assert!(error.to_string().contains("requires PostgreSQL"));
    }

    #[test]
    fn embedded_shadow_compiler_rejects_unsupported_version_during_configuration() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), ..Default::default() },
            shadow: crate::ShadowArgs {
                pg_version: Some(13),
                ..Default::default()
            },
            ..Config::default()
        };

        let error = EmbeddedShadowDBCompiler::from_config(&config)
            .expect_err("unsupported major version should fail");

        assert!(
            error
                .to_string()
                .contains("Supported versions: 14, 15, 16, 17, 18")
        );
    }

    #[test]
    fn compiler_selector_uses_embedded_when_shadow_url_is_missing() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), ..Default::default() },
            ..Config::default()
        };

        compiler_from_config(&config).expect("embedded compiler should configure");
    }

    #[test]
    fn compiler_selector_uses_external_when_shadow_url_is_configured() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), database_url: Some("postgres://localhost/app".to_string()), ..Default::default() },
            shadow: crate::ShadowArgs {
                shadow_database_url: Some("postgres://localhost/shadow".to_string()),
                ..Default::default()
            },
            ..Config::default()
        };

        compiler_from_config(&config).expect("external compiler should configure");
    }

    #[test]
    fn embedded_shadow_compiler_accepts_supported_postgres_major_versions() {
        for version in 14..=18 {
            postgres_major_version_req(version).expect("supported major version should parse");
        }
    }

    #[test]
    fn embedded_shadow_settings_uses_longer_startup_timeout_floor() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), ..Default::default() },
            timeout_seconds: 2,
            ..Config::default()
        };

        let settings = embedded_shadow_settings(&config).expect("settings should build");

        assert_eq!(
            settings.timeout,
            Some(std::time::Duration::from_secs(
                MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS
            ))
        );
    }

    #[test]
    fn embedded_shadow_settings_preserves_longer_configured_timeout() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), ..Default::default() },
            timeout_seconds: MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS + 30,
            ..Config::default()
        };

        let settings = embedded_shadow_settings(&config).expect("settings should build");

        assert_eq!(
            settings.timeout,
            Some(std::time::Duration::from_secs(
                MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS + 30
            ))
        );
    }

    #[test]
    fn embedded_shadow_startup_message_explains_first_run_setup() {
        let message = first_start_message(MIN_EMBEDDED_SHADOW_TIMEOUT_SECONDS);

        assert!(message.contains("First use may download/install PostgreSQL"));
        assert!(message.contains("waiting up to 120s"));
    }

    #[test]
    fn embedded_shadow_first_run_message_only_prints_before_install() {
        assert!(is_embedded_not_installed(Status::NotInstalled));
        assert!(!is_embedded_not_installed(Status::Installed));
        assert!(!is_embedded_not_installed(Status::Stopped));
        assert!(!is_embedded_not_installed(Status::Started));
    }

    #[test]
    fn embedded_shadow_compiler_rejects_unsupported_postgres_major_version() {
        let error =
            postgres_major_version_req(13).expect_err("unsupported major version should fail");

        assert!(
            error
                .to_string()
                .contains("Supported versions: 14, 15, 16, 17, 18")
        );
    }

    #[test]
    fn external_shadow_compiler_requires_shadow_database_url() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), ..Default::default() },
            ..Config::default()
        };

        let error = ExternalShadowDBCompiler::from_config(&config)
            .expect_err("missing shadow database url should fail");

        assert!(
            error
                .to_string()
                .contains("shadow_database_url is required")
        );
    }

    #[test]
    fn external_shadow_compiler_rejects_live_database_url() {
        let config = Config {
            common: crate::CommonArgs { dialect: Some(SqlDialect::Postgres), database_url: Some("postgres://localhost/app".to_string()), ..Default::default() },
            shadow: crate::ShadowArgs {
                shadow_database_url: Some("postgres://localhost/app".to_string()),
                ..Default::default()
            },
            ..Config::default()
        };

        let error = ExternalShadowDBCompiler::from_config(&config)
            .expect_err("matching live and shadow urls should fail");

        assert!(error.to_string().contains("must not be the same"));
    }

    #[test]
    fn external_shadow_owner_marker_is_stable() {
        assert_eq!(EXTERNAL_SHADOW_OWNER_MARKER, "shki:shadow");
    }
}
