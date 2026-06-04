use async_trait::async_trait;
use sqlx::Executor;

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
        Ok(sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .connect(&self.shadow_database_url)
            .await?)
    }
}

#[async_trait]
impl SchemaCompiler for ExternalShadowDBCompiler {
    async fn compile(&self, config: &Config) -> Result<Snapshot> {
        let schema = load_declarative_schema(config.schema_path())?;
        let pool = self.connect(config).await?;

        reset_shadow_database(&pool).await?;
        sqlx::raw_sql(&schema.sql)
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
