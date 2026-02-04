mod dialect;

use crate::{Config, Result, SchemaDialect, ShkiError};
use dialect::{mysql, pg, sqlite};
use sqlx::{Pool, mysql::MySql, postgres::Postgres, sqlite::Sqlite};

/// A unified database pool that can represent pools for different databases.
pub enum DatabasePool {
    Postgres(Pool<Postgres>),
    Mysql(Pool<MySql>),
    Sqlite(Pool<Sqlite>),
}

/// A macro to dispatch operations based on the database pool type.
macro_rules! dispatch_pool {
    ($self:expr, $pool:ident => $body:expr) => {
        match $self {
            DatabasePool::Postgres($pool) => $body,
            DatabasePool::Mysql($pool) => $body,
            DatabasePool::Sqlite($pool) => $body,
        }
    };
}

/// A unified transaction type that can represent transactions for different databases.
pub enum Transaction<'a> {
    Postgres(sqlx::Transaction<'a, Postgres>),
    Mysql(sqlx::Transaction<'a, MySql>),
    Sqlite(sqlx::Transaction<'a, Sqlite>),
}

impl<'a> Transaction<'a> {
    /// Commit the transaction.
    pub async fn commit(self) -> Result<()> {
        match self {
            Transaction::Postgres(tx) => {
                tx.commit().await.map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Mysql(tx) => {
                tx.commit().await.map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Sqlite(tx) => {
                tx.commit().await.map_err(ShkiError::database)?;
                Ok(())
            }
        }
    }

    /// Rollback the transaction.
    pub async fn rollback(self) -> Result<()> {
        match self {
            Transaction::Postgres(tx) => {
                tx.rollback().await.map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Mysql(tx) => {
                tx.rollback().await.map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Sqlite(tx) => {
                tx.rollback().await.map_err(ShkiError::database)?;
                Ok(())
            }
        }
    }

    /// Use SeaQL builders to bind values and build queries safely.
    pub async fn raw_sql(&mut self, query: &str) -> Result<()> {
        match self {
            Transaction::Postgres(tx) => {
                let tx = &mut **tx;
                sqlx::raw_sql(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Mysql(tx) => {
                let tx = &mut **tx;
                sqlx::raw_sql(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Sqlite(tx) => {
                let tx = &mut **tx;
                sqlx::raw_sql(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
        }
    }

    pub async fn query(&mut self, query: &str) -> Result<()> {
        match self {
            Transaction::Postgres(tx) => {
                let tx = &mut **tx;
                sqlx::query(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Mysql(tx) => {
                let tx = &mut **tx;
                sqlx::query(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
            Transaction::Sqlite(tx) => {
                let tx = &mut **tx;
                sqlx::query(query)
                    .execute(tx)
                    .await
                    .map_err(ShkiError::database)?;
                Ok(())
            }
        }
    }
}

/// Implement methods for DatabasePool to execute queries and manage transactions.
///
///
/// Instead of binding values directly in raw SQL strings. Use SeaQL builders to bind values
/// and build queries safely, then execute them using the provided methods.
///
/// # Examples
/// ```rust
/// use shki::db::{create_pool, DatabasePool};
/// use shki::Config;
///
/// // TODO:
///
/// ```
///
impl DatabasePool {
    pub async fn execute(&self, query: &str) -> Result<()> {
        dispatch_pool!(self, pool => {
            sqlx::query(query).execute(pool).await?;
            Ok(())
        })
    }

    /// Begin a new transaction.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use shki::db::{create_pool, DatabasePool};
    /// use shki::Config;
    ///
    /// #[tokio::main]
    /// async fn main() -> shki::Result<()> {
    ///     let config = Config::default();
    ///     let db_pool = create_pool(&config).await?;
    ///     let mut transaction = db_pool.begin().await?;
    ///     // Perform database operations within the transaction
    ///     transaction.raw_sql("INSERT INTO users (name) VALUES ('Alice')").await?;
    ///     transaction.commit().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn begin(&self) -> Result<Transaction<'_>> {
        match self {
            DatabasePool::Postgres(pool) => {
                let tx = pool.begin().await.map_err(ShkiError::database)?;
                Ok(Transaction::Postgres(tx))
            }
            DatabasePool::Mysql(pool) => {
                let tx = pool.begin().await.map_err(ShkiError::database)?;
                Ok(Transaction::Mysql(tx))
            }
            DatabasePool::Sqlite(pool) => {
                let tx = pool.begin().await.map_err(ShkiError::database)?;
                Ok(Transaction::Sqlite(tx))
            }
        }
    }

    // Execute a closure within a transaction.
    // FIX:ME: not sure if this is needed - or has nice DX
    // pub async fn with_transaction<F, Fut, R>(&self, f: F) -> Result<R>
    // where
    //     F: FnOnce(&mut Transaction<'_>) -> Fut + Send,
    //     Fut: std::future::Future<Output = Result<R>> + Send,
    // {
    //     let mut tx = self.begin().await?;
    //     let result = f(&mut tx).await;
    //     match result {
    //         Ok(val) => {
    //             tx.commit().await?;
    //             Ok(val)
    //         }
    //         Err(e) => {
    //             tx.rollback().await?;
    //             Err(e)
    //         }
    //     }
    // }

    /// Execute a raw SQL query.
    pub async fn raw_sql(&self, query: &str) -> Result<()> {
        dispatch_pool!(self, pool => {
            sqlx::raw_sql(query).execute(pool).await?;
            Ok(())
        })
    }

    /// Fetch all records from the database.
    ///
    /// Bound values need to be handled outside of this function.
    /// As this does not expose the `bind` functionality of sqlx,
    pub async fn fetch_all<O>(&self, query: &str) -> Result<Vec<O>>
    where
        O: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>
            + for<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow>
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>
            + Send
            + Unpin,
    {
        dispatch_pool!(self, pool => {
            sqlx::query_as(query).fetch_all(pool).await.map_err(ShkiError::database)
        })
    }

    /// Fetch a single record from the database.
    ///
    /// Bound values need to be handled outside of this function.
    /// As this does not expose the `bind` functionality of sqlx,
    pub async fn fetch_one<O>(&self, query: &str) -> Result<O>
    where
        O: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>
            + for<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow>
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>
            + Send
            + Unpin,
    {
        dispatch_pool!(self, pool => {
            sqlx::query_as(query).fetch_one(pool).await.map_err(ShkiError::database)
        })
    }

    /// Fetch an optional record from the database.
    ///
    /// Bound values need to be handled outside of this function.
    /// As this does not expose the `bind` functionality of sqlx,
    pub async fn fetch_optional<O>(&self, query: &str) -> Result<Option<O>>
    where
        O: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>
            + for<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow>
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>
            + Send
            + Unpin,
    {
        dispatch_pool!(self, pool => {
            sqlx::query_as(query).fetch_optional(pool).await.map_err(ShkiError::database)
        })
    }
}

pub async fn create_pool(config: &Config) -> Result<DatabasePool> {
    match config.dialect {
        SchemaDialect::Postgres => {
            let pool = pg::create_pool(config).await?;
            Ok(DatabasePool::Postgres(pool))
        }
        SchemaDialect::Mysql => {
            let pool = mysql::create_pool(config).await?;
            Ok(DatabasePool::Mysql(pool))
        }
        SchemaDialect::Sqlite => {
            let pool = sqlite::create_pool(config).await?;
            Ok(DatabasePool::Sqlite(pool))
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::MigrationRow;

    use sqlx::Row;

    use super::*;

    #[tokio::test]
    async fn test_database_pool() {
        let config = crate::Config::default();
        let db_pool = create_pool(&config).await.unwrap();
        let tx = db_pool.begin().await.unwrap();

        let rows = match tx {
            Transaction::Postgres(mut tx) => {
                let rows = sqlx::query("SELECT * from __shki_migrations;")
                    .fetch_all(&mut *tx)
                    .await
                    .unwrap();
                rows.iter()
                    .map(|row| MigrationRow {
                        id: row.get("id"),
                        name: row.get("name"),
                        applied_at: row.get("applied_at"),
                    })
                    .collect::<Vec<_>>()
            }
            Transaction::Mysql(mut tx) => {
                let rows = sqlx::query("SELECT * from __shki_migrations;")
                    .fetch_all(&mut *tx)
                    .await
                    .unwrap();
                rows.iter()
                    .map(|row| MigrationRow {
                        id: row.get("id"),
                        name: row.get("name"),
                        applied_at: row.get("applied_at"),
                    })
                    .collect::<Vec<MigrationRow>>()
            }
            Transaction::Sqlite(mut tx) => {
                let rows = sqlx::query("SELECT * from __shki_migrations;")
                    .fetch_all(&mut *tx)
                    .await
                    .unwrap();
                rows.iter()
                    .map(|row| MigrationRow {
                        id: row.get("id"),
                        name: row.get("name"),
                        applied_at: row.get("applied_at"),
                    })
                    .collect::<Vec<MigrationRow>>()
            }
        };

        tx.commit().await.unwrap();
    }
}
