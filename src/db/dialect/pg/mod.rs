//! PostgreSQL database dialect implementation.
//!
//! This module provides PostgreSQL-specific database functionality, including
//! connection pool creation and configuration.
//!
//! # Features
//!
//! - Full-featured relational database
//! - Advanced data types (JSON, arrays, etc.)
//! - Robust transaction support
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
//!     config.database_url = Some("postgres://user:pass@localhost/db".to_string());
//!     
//!     let pool = create_pool(&config).await?;
//!     Ok(())
//! }
//! ```

mod pool;
pub use pool::*;
