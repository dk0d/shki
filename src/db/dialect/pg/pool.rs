use crate::config::Config;
use crate::{Result, ShkiError};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::time::Duration;

/// Create a PostgreSQL connection pool based on the provided configuration.
pub async fn create_pool(config: &Config) -> Result<Pool<Postgres>> {
    let url = config.database_url.clone().ok_or(ShkiError::config(
        "Database URL must be set in order to connect",
    ))?;
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(config.timeout_seconds))
        .connect(&url)
        .await
        .map_err(ShkiError::database)
}
