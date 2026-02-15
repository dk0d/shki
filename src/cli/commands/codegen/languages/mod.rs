//! Language-specific code generators
//!
//! This module contains generators for different output languages.
//! Each generator converts database schema snapshots into language-specific code.

pub mod rust;

pub use rust::RustGenerator;
