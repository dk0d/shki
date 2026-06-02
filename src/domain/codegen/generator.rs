//! Code generator trait
//!
//! This module provides a common trait for code generators that convert
//! database schema snapshots into language-specific code.

use heck::ToUpperCamelCase;
use indexmap::IndexMap;

use crate::models::iden::Iden;
use crate::schema::{DataType, DbEnum, Table};
use crate::snapshots::Snapshot;

use super::CodegenConfig;

/// Singularize a table name (e.g., "users" -> "user")
pub fn singularize(name: &str) -> String {
    pluralizer::pluralize(name, 1, false)
}

/// Trait for code generators that convert database schemas to language-specific code.
///
/// This trait provides default implementations for common naming transformations
/// (struct/enum renames, prefixes, suffixes) that are shared across generators.
pub trait CodeGenerator: Default {
    /// The output type produced by this generator
    type Output;

    /// Language-specific enum representation
    type EnumDef;

    /// Language-specific table/message representation
    type TableDef;

    /// Create an empty output container
    fn init_output(&self, config: &CodegenConfig) -> Self::Output;

    /// Build one enum definition from schema data
    fn generate_enum(
        &self,
        name: &Iden,
        enum_snapshot: &DbEnum,
        config: &CodegenConfig,
    ) -> Self::EnumDef;

    /// Build one table/message definition from schema data
    fn generate_table(
        &self,
        name: &Iden,
        table_snapshot: &Table,
        snapshot: &Snapshot,
        config: &CodegenConfig,
    ) -> Self::TableDef;

    /// Insert a generated enum into output
    fn insert_enum(&self, output: &mut Self::Output, name: &Iden, def: Self::EnumDef);

    /// Insert a generated table/message into output
    fn insert_table(&self, output: &mut Self::Output, name: &Iden, def: Self::TableDef);

    /// Generate code from a schema snapshot
    fn generate(&self, snapshot: &Snapshot, config: &CodegenConfig) -> Self::Output {
        let mut output = self.init_output(config);
        let enums = snapshot.enums();
        let tables = snapshot.tables();

        for (name, enum_snapshot) in &enums {
            let generated_enum = self.generate_enum(name, enum_snapshot, config);
            self.insert_enum(&mut output, name, generated_enum);
        }

        for (table, table_snapshot) in &tables {
            if !config.should_include_table(&table.name) {
                continue;
            }

            let generated_table = self.generate_table(table, table_snapshot, snapshot, config);
            self.insert_table(&mut output, table, generated_table);
        }

        output
    }

    /// Resolve type override if present
    fn overridden_type<'a>(
        &self,
        sql_type: &DataType,
        config: &'a CodegenConfig,
    ) -> Option<&'a String> {
        config.type_overrides.get(&self.type_override_key(sql_type))
    }

    fn type_override_key(&self, sql_type: &DataType) -> String {
        match sql_type {
            DataType::Enum { name, schema } | DataType::Custom { name, schema } => schema
                .as_ref()
                .map(|schema| format!("{}.{}", schema, name))
                .unwrap_or_else(|| name.clone()),
            _ => sql_type.to_postgres_sql().to_lowercase(),
        }
    }

    fn enum_type_name(
        &self,
        name: &str,
        schema: &Option<String>,
        enums: &IndexMap<Iden, DbEnum>,
        config: &CodegenConfig,
    ) -> Option<String> {
        enums
            .contains_key(&Iden::new(name, schema.clone()))
            .then(|| self.transform_enum_name(name, config))
    }

    /// Transform a table name into a struct/message name.
    ///
    /// This applies the following transformations in order:
    /// 1. Check for explicit rename in config
    /// 2. Otherwise, singularize and convert to PascalCase
    /// 3. Apply prefix if configured
    /// 4. Apply suffix if configured
    fn transform_struct_name(&self, name: &str, config: &CodegenConfig) -> String {
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
    fn transform_enum_name(&self, name: &str, config: &CodegenConfig) -> String {
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
    #[derive(Default)]
    struct TestGenerator;

    impl TestGenerator {
        pub fn new() -> Self {
            Self {}
        }
    }

    impl CodeGenerator for TestGenerator {
        type Output = ();
        type EnumDef = ();
        type TableDef = ();

        fn init_output(&self, _config: &CodegenConfig) -> Self::Output {}

        fn generate_enum(
            &self,
            _name: &Iden,
            _enum_snapshot: &DbEnum,
            _config: &CodegenConfig,
        ) -> Self::EnumDef {
        }

        fn generate_table(
            &self,
            _name: &Iden,
            _table_snapshot: &Table,
            _snapshot: &Snapshot,
            _config: &CodegenConfig,
        ) -> Self::TableDef {
        }

        fn insert_enum(&self, _output: &mut Self::Output, _name: &Iden, _def: Self::EnumDef) {}

        fn insert_table(&self, _output: &mut Self::Output, _name: &Iden, _def: Self::TableDef) {}
    }

    #[test]
    fn test_transform_struct_name_default() {
        let config = CodegenConfig::default();

        let generator = TestGenerator::new();

        // Basic transformation: pluralized name -> singular PascalCase
        assert_eq!(generator.transform_struct_name("users", &config), "User");
        assert_eq!(
            generator.transform_struct_name("order_items", &config),
            "OrderItem"
        );
    }

    #[test]
    fn test_transform_struct_name_with_rename() {
        let mut config = CodegenConfig::default();
        config
            .struct_renames
            .insert("users".to_string(), "Person".to_string());

        let generator = TestGenerator::new();
        assert_eq!(generator.transform_struct_name("users", &config), "Person");
    }

    #[test]
    fn test_transform_struct_name_with_prefix() {
        let config = CodegenConfig::default().struct_prefix(Some("Db".to_string()));

        let generator = TestGenerator::new();
        assert_eq!(generator.transform_struct_name("users", &config), "DbUser");
    }

    #[test]
    fn test_transform_struct_name_with_suffix() {
        let config = CodegenConfig::default().struct_suffix(Some("Entity".to_string()));
        let generator = TestGenerator::new();

        assert_eq!(
            generator.transform_struct_name("users", &config),
            "UserEntity"
        );
    }

    #[test]
    fn test_transform_struct_name_with_prefix_and_suffix() {
        let config = CodegenConfig::default()
            .struct_prefix(Some("Db".to_string()))
            .struct_suffix(Some("Entity".to_string()));
        let generator = TestGenerator::new();

        assert_eq!(
            generator.transform_struct_name("users", &config),
            "DbUserEntity"
        );
    }

    #[test]
    fn test_transform_enum_name_default() {
        let config = CodegenConfig::default();

        let generator = TestGenerator::new();
        assert_eq!(
            generator.transform_enum_name("user_status", &config),
            "UserStatus"
        );
        assert_eq!(
            generator.transform_enum_name("order_type", &config),
            "OrderType"
        );
    }

    #[test]
    fn test_transform_enum_name_with_rename() {
        let mut config = CodegenConfig::default();
        config
            .enum_renames
            .insert("status".to_string(), "UserState".to_string());

        let generator = TestGenerator::new();
        assert_eq!(
            generator.transform_enum_name("status", &config),
            "UserState"
        );
    }

    #[test]
    fn test_transform_enum_name_with_prefix_and_suffix() {
        let config = CodegenConfig::default()
            .enum_prefix(Some("Db".to_string()))
            .enum_suffix(Some("Type".to_string()));
        let generator = TestGenerator::new();

        assert_eq!(
            generator.transform_enum_name("user_status", &config),
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
