//! Code generator trait
//!
//! This module provides a common trait for code generators that convert
//! database schema snapshots into language-specific code.

use heck::ToUpperCamelCase;

use crate::commands::codegen::CodegenConfig;
use crate::snapshot::Snapshot;

/// Singularize a table name (e.g., "users" -> "user")
pub fn singularize(name: &str) -> String {
    pluralizer::pluralize(name, 1, false)
}

/// Trait for code generators that convert database schemas to language-specific code.
///
/// This trait provides default implementations for common naming transformations
/// (struct/enum renames, prefixes, suffixes) that are shared across generators.
pub trait CodeGenerator {
    /// The output type produced by this generator
    type Output;

    /// Generate code from a schema snapshot
    fn generate(snapshot: &Snapshot, config: &CodegenConfig) -> Self::Output;

    /// Transform a table name into a struct/message name.
    ///
    /// This applies the following transformations in order:
    /// 1. Check for explicit rename in config
    /// 2. Otherwise, singularize and convert to PascalCase
    /// 3. Apply prefix if configured
    /// 4. Apply suffix if configured
    fn transform_struct_name(name: &str, config: &CodegenConfig) -> String {
        let base_name = config
            .struct_renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| singularize(name).to_upper_camel_case());

        let with_prefix = config
            .struct_prefix
            .as_ref()
            .map(|p| singularize(&format!("{}_{}", p, base_name)).to_upper_camel_case())
            .unwrap_or(base_name);

        config
            .struct_suffix
            .as_ref()
            .map(|s| singularize(&format!("{}_{}", with_prefix, s)).to_upper_camel_case())
            .unwrap_or(with_prefix)
    }

    /// Transform a database enum name into a language enum name.
    ///
    /// This applies the following transformations in order:
    /// 1. Check for explicit rename in config
    /// 2. Otherwise, convert to PascalCase
    /// 3. Apply prefix if configured
    /// 4. Apply suffix if configured
    fn transform_enum_name(name: &str, config: &CodegenConfig) -> String {
        let base_name = config
            .enum_renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_upper_camel_case());

        let with_prefix = config
            .enum_prefix
            .as_ref()
            .map(|p| singularize(&format!("{}_{}", p, base_name)).to_upper_camel_case())
            .unwrap_or(base_name);

        config
            .enum_suffix
            .as_ref()
            .map(|s| singularize(&format!("{}_{}", with_prefix, s)).to_upper_camel_case())
            .unwrap_or(with_prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A simple test generator to verify trait default implementations
    struct TestGenerator;

    impl CodeGenerator for TestGenerator {
        type Output = ();

        fn generate(_snapshot: &Snapshot, _config: &CodegenConfig) -> Self::Output {}
    }

    #[test]
    fn test_transform_struct_name_default() {
        let config = CodegenConfig::default();

        // Basic transformation: pluralized name -> singular PascalCase
        assert_eq!(
            TestGenerator::transform_struct_name("users", &config),
            "User"
        );
        assert_eq!(
            TestGenerator::transform_struct_name("order_items", &config),
            "OrderItem"
        );
    }

    #[test]
    fn test_transform_struct_name_with_rename() {
        let mut config = CodegenConfig::default();
        config
            .struct_renames
            .insert("users".to_string(), "Person".to_string());

        assert_eq!(
            TestGenerator::transform_struct_name("users", &config),
            "Person"
        );
    }

    #[test]
    fn test_transform_struct_name_with_prefix() {
        let config = CodegenConfig::default().struct_prefix(Some("Db".to_string()));

        assert_eq!(
            TestGenerator::transform_struct_name("users", &config),
            "DbUser"
        );
    }

    #[test]
    fn test_transform_struct_name_with_suffix() {
        let config = CodegenConfig::default().struct_suffix(Some("Entity".to_string()));

        assert_eq!(
            TestGenerator::transform_struct_name("users", &config),
            "UserEntity"
        );
    }

    #[test]
    fn test_transform_struct_name_with_prefix_and_suffix() {
        let config = CodegenConfig::default()
            .struct_prefix(Some("Db".to_string()))
            .struct_suffix(Some("Entity".to_string()));

        assert_eq!(
            TestGenerator::transform_struct_name("users", &config),
            "DbUserEntity"
        );
    }

    #[test]
    fn test_transform_enum_name_default() {
        let config = CodegenConfig::default();

        assert_eq!(
            TestGenerator::transform_enum_name("user_status", &config),
            "UserStatus"
        );
        assert_eq!(
            TestGenerator::transform_enum_name("order_type", &config),
            "OrderType"
        );
    }

    #[test]
    fn test_transform_enum_name_with_rename() {
        let mut config = CodegenConfig::default();
        config
            .enum_renames
            .insert("status".to_string(), "UserState".to_string());

        assert_eq!(
            TestGenerator::transform_enum_name("status", &config),
            "UserState"
        );
    }

    #[test]
    fn test_transform_enum_name_with_prefix_and_suffix() {
        let config = CodegenConfig::default()
            .enum_prefix(Some("Db".to_string()))
            .enum_suffix(Some("Type".to_string()));

        assert_eq!(
            TestGenerator::transform_enum_name("user_status", &config),
            "DbUserStatusType"
        );
    }

    #[test]
    fn test_singularize() {
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("orders"), "order");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("user"), "user"); // Already singular
    }
}
