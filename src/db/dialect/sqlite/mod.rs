//! SQLite database dialect implementation.
//!
//! This module provides SQLite-specific database functionality, including
//! connection pool creation and configuration.
//!
//! # Features
//!
//! - Lightweight, file-based database
//! - Zero-configuration deployment
//! - Full ACID compliance
//!
//! # Example
//!
//! ```rust,no_run
//! use shki::Config;
//! use shki::db::create_pool;
//!
//! #[tokio::main]
//! async fn main() -> shki::Result<()> {
//!     let mut config = Config::default();
//!     config.database_url = Some("sqlite::memory:".to_string());
//!     
//!     let pool = create_pool(&config).await?;
//!     Ok(())
//! }
//! ```

mod pool;
pub use pool::*;
