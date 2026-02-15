use crate::{Result, ShkiError, queries};
use crate::{config::Config, schema::SchemaDialect, snapshot::Snapshot};

/// Introspect database based on dialect
pub async fn introspect_db(config: &Config) -> Result<Snapshot> {
    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    match config.dialect {
        SchemaDialect::Postgres => {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(2)
                .connect(db_url)
                .await?;
            queries::pg::introspect_postgres(&pool).await
        }
        SchemaDialect::Mysql => {
            let pool = sqlx::mysql::MySqlPoolOptions::new()
                .max_connections(2)
                .connect(db_url)
                .await?;
            queries::mysql::introspect_mysql(&pool).await
        }
        SchemaDialect::Sqlite => {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(2)
                .connect(db_url)
                .await?;
            queries::sqlite::introspect_sqlite(&pool).await
        }
    }
}
