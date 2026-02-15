//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::diff::*;
use crate::schema::SchemaDialect;
use crate::snapshot::{
    ColumnSnapshot, ConstraintSnapshot, ConstraintType, IndexSnapshot, SequenceSnapshot,
    TableSnapshot, ViewSnapshot,
};
use crate::Result;
use crate::MIGRATION_SPLIT_MARKER;

/// SQL generator for a specific dialect
pub struct SqlGenerator {
    dialect: SchemaDialect,
    breakpoints: bool,
}

impl SqlGenerator {
    /// Create a new SQL generator
    pub fn new(dialect: SchemaDialect) -> Self {
        Self {
            dialect,
            breakpoints: true,
        }
    }

    /// Set whether to include breakpoints
    pub fn with_breakpoints(mut self, breakpoints: bool) -> Self {
        self.breakpoints = breakpoints;
        self
    }

    /// Generate SQL for a diff
    pub fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>> {
        let mut statements = Vec::new();

        for stmt in &diff.statements {
            let sql = self.generate_statement(stmt)?;
            statements.extend(sql);
        }

        Ok(statements)
    }

    /// Generate SQL for a single statement
    pub fn generate_statement(&self, stmt: &DiffStatement) -> Result<Vec<String>> {
        match stmt {
            DiffStatement::CreateSchema { name } => Ok(vec![self.create_schema(name)]),
            DiffStatement::DropSchema { name, cascade } => {
                Ok(vec![self.drop_schema(name, *cascade)])
            }
            DiffStatement::RenameSchema { from, to } => Ok(vec![self.rename_schema(from, to)]),
            DiffStatement::CreateEnum {
                name,
                schema,
                values,
                description,
            } => Ok(self.create_enum(name, schema, values, description)),
            DiffStatement::DropEnum { name, schema, .. } => Ok(vec![self.drop_enum(name, schema)]),
            DiffStatement::RenameEnum { from, to, schema } => {
                Ok(vec![self.rename_enum(from, to, schema)])
            }
            DiffStatement::AddEnumValue {
                enum_name,
                schema,
                value,
                position,
            } => Ok(vec![self.add_enum_value(enum_name, schema, value, position)]),
            DiffStatement::AlterEnumDescription {
                name,
                schema,
                description,
                ..
            } => Ok(vec![self.alter_enum_description(name, schema, description)]),
            DiffStatement::CreateSequence { sequence } => Ok(vec![self.create_sequence(sequence)]),
            DiffStatement::DropSequence { name, schema, .. } => {
                Ok(vec![self.drop_sequence(name, schema)])
            }
            DiffStatement::AlterSequence {
                name,
                schema,
                changes,
            } => Ok(vec![self.alter_sequence(name, schema, changes)]),
            DiffStatement::CreateTable { table } => Ok(self.create_table(table)),
            DiffStatement::DropTable {
                name,
                schema,
                cascade,
                ..
            } => Ok(vec![self.drop_table(name, schema, *cascade)]),
            DiffStatement::RenameTable { from, to, schema } => {
                Ok(vec![self.rename_table(from, to, schema)])
            }
            DiffStatement::AlterTableComment {
                table,
                schema,
                comment,
                // don't need prev to set the comment -
                // only used to build down migration
                prev: _,
            } => Ok(vec![self.alter_table_comment(table, schema, comment)]),
            DiffStatement::AddColumn {
                table,
                schema,
                column,
            } => Ok(vec![self.add_column(table, schema, column)]),
            DiffStatement::DropColumn {
                table,
                schema,
                column,
                cascade,
                ..
            } => Ok(vec![self.drop_column(table, schema, column, *cascade)]),
            DiffStatement::RenameColumn {
                table,
                schema,
                from,
                to,
            } => Ok(vec![self.rename_column(table, schema, from, to)]),
            DiffStatement::AlterColumn {
                table,
                schema,
                column,
                changes,
            } => Ok(self.alter_column(table, schema, column, changes)),
            DiffStatement::AlterColumnComment {
                table,
                schema,
                column,
                comment,
                ..
            } => Ok(vec![
                self.alter_column_comment(table, schema, column, comment)
            ]),
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
                concurrently,
                if_not_exists,
            } => Ok(vec![self.create_index(
                table,
                schema,
                index,
                *concurrently,
                *if_not_exists,
            )]),
            DiffStatement::DropIndex {
                name,
                schema,
                concurrently,
                if_exists,
                ..
            } => Ok(vec![self.drop_index(
                name,
                schema,
                *concurrently,
                *if_exists,
            )]),
            DiffStatement::AddConstraint {
                table,
                schema,
                constraint,
            } => Ok(vec![self.add_constraint(table, schema, constraint)]),
            DiffStatement::DropConstraint {
                table,
                schema,
                name,
                cascade,
                ..
            } => Ok(vec![self.drop_constraint(table, schema, name, *cascade)]),
            DiffStatement::CreateView { view, or_replace } => {
                Ok(vec![self.create_view(view, *or_replace)])
            }
            DiffStatement::DropView {
                name,
                schema,
                materialized,
                cascade,
                ..
            } => Ok(vec![self.drop_view(name, schema, *materialized, *cascade)]),
            DiffStatement::AlterView {
                name,
                schema,
                new_definition,
                ..
            } => Ok(vec![self.alter_view(name, schema, new_definition)]),
            DiffStatement::CreateExtension(name) => Ok(vec![self.create_extension(name)]),
            DiffStatement::DropExtension(name) => Ok(vec![self.drop_extension(name)]),
        }
    }

    /// Generate combined SQL with breakpoints
    pub fn generate_sql(&self, diff: &SchemaDiff) -> Result<String> {
        let statements = self.generate(diff)?;

        if self.breakpoints {
            Ok(statements.join(&format!(";\n{}\n", MIGRATION_SPLIT_MARKER)) + ";")
        } else {
            Ok(statements.join(";\n") + ";")
        }
    }

    // Helper methods

    fn quote_identifier(&self, name: &str) -> String {
        match self.dialect {
            SchemaDialect::Postgres | SchemaDialect::Sqlite => {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
            SchemaDialect::Mysql => {
                format!("`{}`", name.replace('`', "``"))
            }
        }
    }

    fn qualified_name(&self, name: &str, schema: &Option<String>) -> String {
        match schema {
            Some(s) => format!(
                "{}.{}",
                self.quote_identifier(s),
                self.quote_identifier(name)
            ),
            None => self.quote_identifier(name),
        }
    }

    // Schema operations

    fn create_schema(&self, name: &str) -> String {
        format!("CREATE SCHEMA {}", self.quote_identifier(name))
    }

    fn drop_schema(&self, name: &str, cascade: bool) -> String {
        let cascade = if cascade { " CASCADE" } else { "" };
        format!("DROP SCHEMA {}{}", self.quote_identifier(name), cascade)
    }

    fn rename_schema(&self, from: &str, to: &str) -> String {
        format!(
            "ALTER SCHEMA {} RENAME TO {}",
            self.quote_identifier(from),
            self.quote_identifier(to)
        )
    }

    // Enum operations (PostgreSQL)

    fn create_enum(
        &self,
        name: &str,
        schema: &Option<String>,
        values: &[String],
        description: &Option<String>,
    ) -> Vec<String> {
        let qualified = self.qualified_name(name, schema);
        let values_str: Vec<String> = values.iter().map(|v| format!("'{}'", v)).collect();
        let mut result = vec![format!(
            "CREATE TYPE {} AS ENUM ({})",
            qualified,
            values_str.join(", ")
        )];

        // Add COMMENT ON TYPE if description is present
        if let Some(desc) = description {
            let escaped = desc.replace('\'', "''");
            result.push(format!("COMMENT ON TYPE {} IS '{}'", qualified, escaped));
        }

        result
    }

    fn drop_enum(&self, name: &str, schema: &Option<String>) -> String {
        let qualified = self.qualified_name(name, schema);
        format!("DROP TYPE {}", qualified)
    }

    fn rename_enum(&self, from: &str, to: &str, schema: &Option<String>) -> String {
        let qualified = self.qualified_name(from, schema);
        format!(
            "ALTER TYPE {} RENAME TO {}",
            qualified,
            self.quote_identifier(to)
        )
    }

    fn add_enum_value(
        &self,
        enum_name: &str,
        schema: &Option<String>,
        value: &str,
        position: &EnumValuePosition,
    ) -> String {
        let qualified = self.qualified_name(enum_name, schema);
        let position_str = match position {
            EnumValuePosition::End => String::new(),
            EnumValuePosition::Before(v) => format!(" BEFORE '{}'", v),
            EnumValuePosition::After(v) => format!(" AFTER '{}'", v),
        };
        format!(
            "ALTER TYPE {} ADD VALUE '{}'{}",
            qualified, value, position_str
        )
    }

    fn alter_enum_description(
        &self,
        name: &str,
        schema: &Option<String>,
        description: &Option<String>,
    ) -> String {
        let qualified = self.qualified_name(name, schema);
        match description {
            Some(desc) => {
                let escaped = desc.replace('\'', "''");
                format!("COMMENT ON TYPE {} IS '{}'", qualified, escaped)
            }
            None => format!("COMMENT ON TYPE {} IS NULL", qualified),
        }
    }

    // Sequence operations

    fn create_sequence(&self, sequence: &SequenceSnapshot) -> String {
        let name = self.qualified_name(&sequence.name, &sequence.schema);
        let mut parts = vec![format!("CREATE SEQUENCE {}", name)];

        parts.push(format!("INCREMENT BY {}", sequence.increment));
        parts.push(format!("MINVALUE {}", sequence.min_value));

        if let Some(max) = sequence.max_value {
            parts.push(format!("MAXVALUE {}", max));
        } else {
            parts.push("NO MAXVALUE".to_string());
        }

        parts.push(format!("START WITH {}", sequence.start));
        parts.push(format!("CACHE {}", sequence.cache));

        if sequence.cycle {
            parts.push("CYCLE".to_string());
        } else {
            parts.push("NO CYCLE".to_string());
        }

        parts.join(" ")
    }

    fn drop_sequence(&self, name: &str, schema: &Option<String>) -> String {
        let qualified = self.qualified_name(name, schema);
        format!("DROP SEQUENCE {}", qualified)
    }

    fn alter_sequence(
        &self,
        name: &str,
        schema: &Option<String>,
        changes: &[SequenceChange],
    ) -> String {
        let qualified = self.qualified_name(name, schema);
        let mut parts = vec![format!("ALTER SEQUENCE {}", qualified)];

        for change in changes {
            match change {
                SequenceChange::Increment(v) => parts.push(format!("INCREMENT BY {}", v)),
                SequenceChange::MinValue(v) => parts.push(format!("MINVALUE {}", v)),
                SequenceChange::MaxValue(Some(v)) => parts.push(format!("MAXVALUE {}", v)),
                SequenceChange::MaxValue(None) => parts.push("NO MAXVALUE".to_string()),
                SequenceChange::Start(v) => parts.push(format!("START WITH {}", v)),
                SequenceChange::Cache(v) => parts.push(format!("CACHE {}", v)),
                SequenceChange::Cycle(true) => parts.push("CYCLE".to_string()),
                SequenceChange::Cycle(false) => parts.push("NO CYCLE".to_string()),
            }
        }

        parts.join(" ")
    }

    // Table operations

    fn create_table(&self, table: &TableSnapshot) -> Vec<String> {
        let name = self.qualified_name(&table.name, &table.schema);

        let mut column_defs: Vec<String> = table
            .columns
            .values()
            .map(|c| self.column_definition(c))
            .collect();

        // Add table-level constraints
        for constraint in &table.constraints {
            column_defs.push(self.constraint_definition(constraint));
        }

        let mut result = vec![format!(
            "CREATE TABLE {} (\n  {}\n)",
            name,
            column_defs.join(",\n  ")
        )];

        // Add COMMENT ON TABLE if comment is present
        if let Some(comment) = &table.comment {
            let escaped = comment.replace('\'', "''");
            result.push(format!("COMMENT ON TABLE {} IS '{}'", name, escaped));
        }

        // Add COMMENT ON COLUMN for columns with comments
        for col in table.columns.values() {
            if let Some(comment) = &col.comment {
                let escaped = comment.replace('\'', "''");
                result.push(format!(
                    "COMMENT ON COLUMN {}.{} IS '{}'",
                    name,
                    self.quote_identifier(&col.name),
                    escaped
                ));
            }
        }

        result
    }

    fn column_definition(&self, col: &ColumnSnapshot) -> String {
        let mut parts = vec![self.quote_identifier(&col.name), col.data_type.clone()];

        if col.primary_key {
            parts.push("PRIMARY KEY".to_string());
        }

        if !col.nullable {
            parts.push("NOT NULL".to_string());
        }

        if col.unique && !col.primary_key {
            parts.push("UNIQUE".to_string());
        }

        if let Some(default) = &col.default {
            parts.push(format!("DEFAULT {}", default));
        }

        if let Some(generated) = &col.generated {
            parts.push(generated.clone());
        }

        if let Some(collation) = &col.collation {
            parts.push(format!("COLLATE {}", self.quote_identifier(collation)));
        }

        parts.join(" ")
    }

    fn constraint_definition(&self, constraint: &ConstraintSnapshot) -> String {
        let mut sql = String::new();

        if let Some(name) = &constraint.name {
            sql.push_str(&format!("CONSTRAINT {} ", self.quote_identifier(name)));
        }

        match constraint.constraint_type {
            ConstraintType::PrimaryKey => {
                let cols: Vec<String> = constraint
                    .columns
                    .iter()
                    .map(|c| self.quote_identifier(c))
                    .collect();
                sql.push_str(&format!("PRIMARY KEY ({})", cols.join(", ")));
            }
            ConstraintType::Unique => {
                let cols: Vec<String> = constraint
                    .columns
                    .iter()
                    .map(|c| self.quote_identifier(c))
                    .collect();
                sql.push_str(&format!("UNIQUE ({})", cols.join(", ")));
            }
            ConstraintType::ForeignKey => {
                if let Some(ref_info) = &constraint.references {
                    let cols: Vec<String> = constraint
                        .columns
                        .iter()
                        .map(|c| self.quote_identifier(c))
                        .collect();
                    let ref_cols: Vec<String> = ref_info
                        .columns
                        .iter()
                        .map(|c| self.quote_identifier(c))
                        .collect();
                    let ref_table = self.qualified_name(&ref_info.table, &ref_info.schema);

                    sql.push_str(&format!(
                        "FOREIGN KEY ({}) REFERENCES {} ({})",
                        cols.join(", "),
                        ref_table,
                        ref_cols.join(", ")
                    ));

                    if ref_info.on_delete != "NO ACTION" {
                        sql.push_str(&format!(" ON DELETE {}", ref_info.on_delete));
                    }
                    if ref_info.on_update != "NO ACTION" {
                        sql.push_str(&format!(" ON UPDATE {}", ref_info.on_update));
                    }
                }
            }
            ConstraintType::Check => {
                if let Some(expr) = &constraint.expression {
                    sql.push_str(&format!("CHECK ({})", expr));
                }
            }
            ConstraintType::Exclusion => {
                // Exclusion constraints are PostgreSQL-specific
                sql.push_str("EXCLUDE USING gist (/* TODO */)");
            }
        }

        sql
    }

    fn drop_table(&self, name: &str, schema: &Option<String>, cascade: bool) -> String {
        let qualified = self.qualified_name(name, schema);
        let cascade_str = if cascade { " CASCADE" } else { "" };
        format!("DROP TABLE {}{}", qualified, cascade_str)
    }

    fn rename_table(&self, from: &str, to: &str, schema: &Option<String>) -> String {
        let qualified = self.qualified_name(from, schema);
        format!(
            "ALTER TABLE {} RENAME TO {}",
            qualified,
            self.quote_identifier(to)
        )
    }

    fn alter_table_comment(
        &self,
        table: &str,
        schema: &Option<String>,
        comment: &Option<String>,
    ) -> String {
        let qualified = self.qualified_name(table, schema);
        match comment {
            Some(c) => {
                let escaped = c.replace('\'', "''");
                format!("COMMENT ON TABLE {} IS '{}'", qualified, escaped)
            }
            None => format!("COMMENT ON TABLE {} IS NULL", qualified),
        }
    }

    // Column operations

    fn add_column(&self, table: &str, schema: &Option<String>, column: &ColumnSnapshot) -> String {
        let qualified = self.qualified_name(table, schema);
        format!(
            "ALTER TABLE {} ADD COLUMN {}",
            qualified,
            self.column_definition(column)
        )
    }

    fn drop_column(
        &self,
        table: &str,
        schema: &Option<String>,
        column: &str,
        cascade: bool,
    ) -> String {
        let qualified = self.qualified_name(table, schema);
        let cascade_str = if cascade { " CASCADE" } else { "" };
        format!(
            "ALTER TABLE {} DROP COLUMN {}{}",
            qualified,
            self.quote_identifier(column),
            cascade_str
        )
    }

    fn rename_column(&self, table: &str, schema: &Option<String>, from: &str, to: &str) -> String {
        let qualified = self.qualified_name(table, schema);
        format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            qualified,
            self.quote_identifier(from),
            self.quote_identifier(to)
        )
    }

    fn alter_column(
        &self,
        table: &str,
        schema: &Option<String>,
        column: &str,
        changes: &[ColumnChange],
    ) -> Vec<String> {
        let qualified = self.qualified_name(table, schema);
        let column_quoted = self.quote_identifier(column);

        changes
            .iter()
            .map(|change| match change {
                ColumnChange::SetType(t) => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                        qualified, column_quoted, t
                    )
                }
                ColumnChange::SetNotNull => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL",
                        qualified, column_quoted
                    )
                }
                ColumnChange::DropNotNull => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                        qualified, column_quoted
                    )
                }
                ColumnChange::SetDefault(d) => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                        qualified, column_quoted, d
                    )
                }
                ColumnChange::DropDefault => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                        qualified, column_quoted
                    )
                }
                ColumnChange::SetGenerated(g) => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET {}",
                        qualified, column_quoted, g
                    )
                }
                ColumnChange::DropGenerated => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP EXPRESSION",
                        qualified, column_quoted
                    )
                }
            })
            .collect()
    }

    fn alter_column_comment(
        &self,
        table: &str,
        schema: &Option<String>,
        column: &str,
        comment: &Option<String>,
    ) -> String {
        let qualified = self.qualified_name(table, schema);
        let column_quoted = self.quote_identifier(column);
        match comment {
            Some(c) => {
                let escaped = c.replace('\'', "''");
                format!(
                    "COMMENT ON COLUMN {}.{} IS '{}'",
                    qualified, column_quoted, escaped
                )
            }
            None => format!("COMMENT ON COLUMN {}.{} IS NULL", qualified, column_quoted),
        }
    }

    // Index operations

    fn create_index(
        &self,
        table: &str,
        schema: &Option<String>,
        index: &IndexSnapshot,
        concurrently: bool,
        if_not_exists: bool,
    ) -> String {
        let mut sql = String::from("CREATE ");

        if index.unique {
            sql.push_str("UNIQUE ");
        }

        sql.push_str("INDEX ");

        if concurrently {
            sql.push_str("CONCURRENTLY ");
        }

        if if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }

        sql.push_str(&self.quote_identifier(&index.name));
        sql.push_str(" ON ");
        sql.push_str(&self.qualified_name(table, schema));

        if index.method != "btree" {
            sql.push_str(&format!(" USING {}", index.method));
        }

        let cols: Vec<String> = index
            .columns
            .iter()
            .map(|c| {
                if c.starts_with('(') {
                    c.clone() // Expression
                } else {
                    self.quote_identifier(c)
                }
            })
            .collect();
        sql.push_str(&format!(" ({})", cols.join(", ")));

        if !index.include.is_empty() {
            let include_cols: Vec<String> = index
                .include
                .iter()
                .map(|c| self.quote_identifier(c))
                .collect();
            sql.push_str(&format!(" INCLUDE ({})", include_cols.join(", ")));
        }

        if let Some(where_clause) = &index.where_clause {
            sql.push_str(&format!(" WHERE {}", where_clause));
        }

        sql
    }

    fn drop_index(
        &self,
        name: &str,
        schema: &Option<String>,
        concurrently: bool,
        if_exists: bool,
    ) -> String {
        let mut sql = String::from("DROP INDEX ");

        if concurrently {
            sql.push_str("CONCURRENTLY ");
        }

        if if_exists {
            sql.push_str("IF EXISTS ");
        }

        sql.push_str(&self.qualified_name(name, schema));
        sql
    }

    // Constraint operations

    fn add_constraint(
        &self,
        table: &str,
        schema: &Option<String>,
        constraint: &ConstraintSnapshot,
    ) -> String {
        let qualified = self.qualified_name(table, schema);
        format!(
            "ALTER TABLE {} ADD {}",
            qualified,
            self.constraint_definition(constraint)
        )
    }

    fn drop_constraint(
        &self,
        table: &str,
        schema: &Option<String>,
        name: &str,
        cascade: bool,
    ) -> String {
        let qualified = self.qualified_name(table, schema);
        let cascade_str = if cascade { " CASCADE" } else { "" };
        format!(
            "ALTER TABLE {} DROP CONSTRAINT {}{}",
            qualified,
            self.quote_identifier(name),
            cascade_str
        )
    }

    // View operations

    fn create_view(&self, view: &ViewSnapshot, or_replace: bool) -> String {
        let name = self.qualified_name(&view.name, &view.schema);

        let mut sql = String::from("CREATE ");

        if or_replace {
            sql.push_str("OR REPLACE ");
        }

        if view.materialized {
            sql.push_str("MATERIALIZED ");
        }

        sql.push_str(&format!("VIEW {} AS {}", name, view.definition));
        sql
    }

    fn drop_view(
        &self,
        name: &str,
        schema: &Option<String>,
        materialized: bool,
        cascade: bool,
    ) -> String {
        let qualified = self.qualified_name(name, schema);
        let materialized_str = if materialized { "MATERIALIZED " } else { "" };
        let cascade_str = if cascade { " CASCADE" } else { "" };
        format!("DROP {}VIEW {}{}", materialized_str, qualified, cascade_str)
    }

    fn alter_view(&self, name: &str, schema: &Option<String>, new_definition: &str) -> String {
        let qualified = self.qualified_name(name, schema);
        format!("CREATE OR REPLACE VIEW {} AS {}", qualified, new_definition)
    }

    // Extension operations (PostgreSQL)

    fn create_extension(&self, name: &str) -> String {
        format!(
            "CREATE EXTENSION IF NOT EXISTS {}",
            self.quote_identifier(name)
        )
    }

    fn drop_extension(&self, name: &str) -> String {
        format!("DROP EXTENSION IF EXISTS {}", self.quote_identifier(name))
    }
}
