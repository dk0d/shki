//! Language-specific code generators
//!
//! This module contains generators for different output languages.
//! Each generator converts database schema snapshots into language-specific code.

mod generator;
pub mod protobuf;
pub mod rust;

pub use generator::{singularize, CodeGenerator};
pub use protobuf::*;
pub use rust::*;
