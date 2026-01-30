use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;

pub async fn create_any_pool(url: &str) -> AnyPool {
    sqlx::any::install_default_drivers();
    AnyPool::connect(url)
        .await
        .expect("Failed to connect to PostgreSQL with AnyPool")
}

pub fn create_any_pool_opts() -> AnyPoolOptions {
    sqlx::any::install_default_drivers();
    AnyPoolOptions::new()
}
