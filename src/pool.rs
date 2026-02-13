use std::time::Duration;

use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;

pub fn create_any_pool_opts() -> AnyPoolOptions {
    sqlx::any::install_default_drivers();
    AnyPoolOptions::new()
}

pub async fn create_any_pool(url: &str) -> AnyPool {
    create_any_pool_opts()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect(url)
        // AnyPool::connect(url)
        .await
        .expect("Failed to connect to PostgreSQL with AnyPool")
}
