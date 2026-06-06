use async_trait::async_trait;
use postgresql_embedded::{PostgreSQL as EmbeddedPostgres, SettingsBuilder, VersionReq};
use sqlx::Executor;
use uuid::Uuid;

use crate::config::Config;
use crate::declarative::load_declarative_schema;
use crate::engines::pg::Postgres;
use crate::schema::SqlDialect;
use crate::snapshots::{Introspectable, Snapshot};

use crate::{Result, ShkiError};

#[async_trait]
pub trait SchemaCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot>;
}

pub fn compiler_from_config(config: &Config) -> Result<Box<dyn SchemaCompiler + Send + Sync>> {
    if config.shadow_database_url.is_some() {
        Ok(Box::new(ExternalShadowDBCompiler::from_config(config)?))
    } else {
        Ok(Box::new(EmbeddedShadowDBCompiler::from_config(config)?))
    }
}

#[derive(Debug, Default)]
pub struct EmbeddedShadowDBCompiler;

impl EmbeddedShadowDBCompiler {
    pub fn from_config(config: &Config) -> Result<Self> {
        if config.dialect != SqlDialect::Postgres {
            return Err(ShkiError::unsupported_dialect(
                "Declarative Schema compilation currently requires PostgreSQL",
            ));
        }
        if let Some(version) = config.pg_version {
            postgres_major_version_req(version)?;
        }

        Ok(Self)
    }
}

#[async_trait]
impl SchemaCompiler for EmbeddedShadowDBCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot> {
        let schema = load_declarative_schema(config.schema_path())?;
        let mut settings = SettingsBuilder::new()
            .timeout(Some(std::time::Duration::from_secs(config.timeout_seconds)));
        settings = settings.version(postgres_major_version_req(config.pg_version.unwrap_or(18))?);
        let mut server = EmbeddedPostgres::new(settings.build());
        server.setup().await.map_err(|err| {
            ShkiError::schema(format!(
                "Failed to set up embedded Shadow Database: {}",
                err
            ))
        })?;
        server.start().await.map_err(|err| {
            ShkiError::schema(format!("Failed to start embedded Shadow Database: {}", err))
        })?;

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
        let result = compile_with_pool(config, &schema.sql, pool).await;
        let stop_result = server.stop().await.map_err(|err| {
            ShkiError::schema(format!("Failed to stop embedded Shadow Database: {}", err))
        });

        match (result, stop_result) {
            (Ok(snapshot), Ok(())) => Ok(snapshot),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }
}

#[derive(Debug)]
pub struct ExternalShadowDBCompiler {
    shadow_database_url: String,
}

impl ExternalShadowDBCompiler {
    pub fn from_config(config: &Config) -> Result<Self> {
        if config.dialect != SqlDialect::Postgres {
            return Err(ShkiError::unsupported_dialect(
                "Declarative Schema compilation currently requires PostgreSQL",
            ));
        }

        let shadow_database_url = config.shadow_database_url.clone().ok_or_else(|| {
            ShkiError::config("shadow_database_url is required to compile a Declarative Schema")
        })?;

        if config.database_url.as_deref() == Some(shadow_database_url.as_str()) {
            return Err(ShkiError::config(
                "shadow_database_url must not be the same as database_url",
            ));
        }

        Ok(Self {
            shadow_database_url,
        })
    }

    async fn connect(&self, config: &Config) -> Result<sqlx::Pool<sqlx::Postgres>> {
        connect_postgres(config, &self.shadow_database_url).await
    }
}

#[async_trait]
impl SchemaCompiler for ExternalShadowDBCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot> {
        let schema = load_declarative_schema(config.schema_path())?;
        let pool = self.connect(config).await?;

        compile_with_pool(config, &schema.sql, pool).await
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
    sqlx::raw_sql(schema_sql)
        .execute(&pool)
        .await
        .map_err(|err| {
            ShkiError::schema(format!(
                "Failed to apply Declarative Schema to Shadow Database: {}",
                err
            ))
        })?;

    let engine = Postgres::new(pool, config.migrations.entity());
    introspect_all_schemas(config, &engine).await
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
            dialect: SqlDialect::Sqlite,
            shadow_database_url: Some("sqlite://shadow".to_string()),
            ..Config::default()
        };

        let error = ExternalShadowDBCompiler::from_config(&config)
            .expect_err("non-postgres compiler config should fail");

        assert!(error.to_string().contains("requires PostgreSQL"));
    }

    #[test]
    fn embedded_shadow_compiler_requires_postgres() {
        let config = Config {
            dialect: SqlDialect::Sqlite,
            ..Config::default()
        };

        let error = EmbeddedShadowDBCompiler::from_config(&config)
            .expect_err("non-postgres compiler config should fail");

        assert!(error.to_string().contains("requires PostgreSQL"));
    }

    #[test]
    fn embedded_shadow_compiler_rejects_unsupported_version_during_configuration() {
        let config = Config {
            dialect: SqlDialect::Postgres,
            pg_version: Some(13),
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
            dialect: SqlDialect::Postgres,
            ..Config::default()
        };

        compiler_from_config(&config).expect("embedded compiler should configure");
    }

    #[test]
    fn compiler_selector_uses_external_when_shadow_url_is_configured() {
        let config = Config {
            dialect: SqlDialect::Postgres,
            database_url: Some("postgres://localhost/app".to_string()),
            shadow_database_url: Some("postgres://localhost/shadow".to_string()),
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
            dialect: SqlDialect::Postgres,
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
            dialect: SqlDialect::Postgres,
            database_url: Some("postgres://localhost/app".to_string()),
            shadow_database_url: Some("postgres://localhost/app".to_string()),
            ..Config::default()
        };

        let error = ExternalShadowDBCompiler::from_config(&config)
            .expect_err("matching live and shadow urls should fail");

        assert!(error.to_string().contains("must not be the same"));
    }
}
