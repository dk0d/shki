//! Language-specific code generators
//!
//! This module contains generators for different output languages.
//! Each generator converts database schema snapshots into language-specific code.

mod generator;
pub mod protobuf;
pub mod rust;
pub mod typescript;

pub use generator::{CodeGenerator, singularize};
pub use protobuf::*;
pub use rust::*;
pub use typescript::*;
