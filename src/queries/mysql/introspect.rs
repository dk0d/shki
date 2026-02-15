use crate::Result;
use crate::Snapshot;
use sqlx::{MySql, Pool};

/// Introspect a MySQL database
pub async fn introspect_mysql(_pool: &Pool<MySql>) -> Result<Snapshot> {
    todo!();
}
