//! MySQL database dialect implementation.
//!
//! This module provides MySQL-specific database functionality, including
//! connection pool creation and configuration.
//!
//! # Features
//!
//! - Popular open-source relational database
//! - Wide ecosystem support
//! - High performance for read-heavy workloads
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
//!     config.database_url = Some("mysql://user:pass@localhost/db".to_string());
//!     
//!     let pool = create_pool(&config).await?;
//!     Ok(())
//! }
//! ```

mod pool;
pub use pool::*;
