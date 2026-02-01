//! Database dialect implementations for different database backends.
//!
//! This module contains database-specific implementations for:
//!
//! - [`mysql`] - MySQL database support
//! - [`pg`] - PostgreSQL database support
//! - [`sqlite`] - SQLite database support
//!
//! Each dialect module provides functions for creating connection pools
//! and executing database-specific operations.

pub mod mysql;
pub mod pg;
pub mod sqlite;
