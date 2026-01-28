//! SQL generation from diff statements
//!
//! This module converts diff statements into executable SQL.

use crate::diff::*;
use crate::schema::SchemaDialect;
use crate::snapshot::{ColumnSnapshot, ConstraintSnapshot, ConstraintType};
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
            DiffStatement::CreateSchema(s) => Ok(vec![self.create_schema(s)]),
            DiffStatement::DropSchema(s) => Ok(vec![self.drop_schema(s)]),
            DiffStatement::RenameSchema(s) => Ok(vec![self.rename_schema(s)]),
            DiffStatement::CreateEnum(s) => Ok(self.create_enum(s)),
            DiffStatement::DropEnum(s) => Ok(vec![self.drop_enum(s)]),
            DiffStatement::RenameEnum(s) => Ok(vec![self.rename_enum(s)]),
            DiffStatement::AddEnumValue(s) => Ok(vec![self.add_enum_value(s)]),
            DiffStatement::AlterEnumDescription(s) => Ok(vec![self.alter_enum_description(s)]),
            DiffStatement::CreateSequence(s) => Ok(vec![self.create_sequence(s)]),
            DiffStatement::DropSequence(s) => Ok(vec![self.drop_sequence(s)]),
            DiffStatement::AlterSequence(s) => Ok(vec![self.alter_sequence(s)]),
            DiffStatement::CreateTable(s) => Ok(self.create_table(s)),
            DiffStatement::DropTable(s) => Ok(vec![self.drop_table(s)]),
            DiffStatement::RenameTable(s) => Ok(vec![self.rename_table(s)]),
            DiffStatement::AlterTableComment(s) => Ok(vec![self.alter_table_comment(s)]),
            DiffStatement::AddColumn(s) => Ok(vec![self.add_column(s)]),
            DiffStatement::DropColumn(s) => Ok(vec![self.drop_column(s)]),
            DiffStatement::RenameColumn(s) => Ok(vec![self.rename_column(s)]),
            DiffStatement::AlterColumn(s) => Ok(self.alter_column(s)),
            DiffStatement::AlterColumnComment(s) => Ok(vec![self.alter_column_comment(s)]),
            DiffStatement::CreateIndex(s) => Ok(vec![self.create_index(s)]),
            DiffStatement::DropIndex(s) => Ok(vec![self.drop_index(s)]),
            DiffStatement::AddConstraint(s) => Ok(vec![self.add_constraint(s)]),
            DiffStatement::DropConstraint(s) => Ok(vec![self.drop_constraint(s)]),
            DiffStatement::CreateView(s) => Ok(vec![self.create_view(s)]),
            DiffStatement::DropView(s) => Ok(vec![self.drop_view(s)]),
            DiffStatement::AlterView(s) => Ok(vec![self.alter_view(s)]),
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

    fn create_schema(&self, stmt: &CreateSchemaStmt) -> String {
        format!("CREATE SCHEMA {}", self.quote_identifier(&stmt.name))
    }

    fn drop_schema(&self, stmt: &DropSchemaStmt) -> String {
        let cascade = if stmt.cascade { " CASCADE" } else { "" };
        format!(
            "DROP SCHEMA {}{}",
            self.quote_identifier(&stmt.name),
            cascade
        )
    }

    fn rename_schema(&self, stmt: &RenameSchemaStmt) -> String {
        format!(
            "ALTER SCHEMA {} RENAME TO {}",
            self.quote_identifier(&stmt.from),
            self.quote_identifier(&stmt.to)
        )
    }

    // Enum operations (PostgreSQL)

    fn create_enum(&self, stmt: &CreateEnumStmt) -> Vec<String> {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        let values: Vec<String> = stmt.values.iter().map(|v| format!("'{}'", v)).collect();
        let mut result = vec![format!(
            "CREATE TYPE {} AS ENUM ({})",
            name,
            values.join(", ")
        )];

        // Add COMMENT ON TYPE if description is present
        if let Some(desc) = &stmt.description {
            let escaped = desc.replace('\'', "''");
            result.push(format!("COMMENT ON TYPE {} IS '{}'", name, escaped));
        }

        result
    }

    fn drop_enum(&self, stmt: &DropEnumStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        format!("DROP TYPE {}", name)
    }

    fn rename_enum(&self, stmt: &RenameEnumStmt) -> String {
        let name = self.qualified_name(&stmt.from, &stmt.schema);
        format!(
            "ALTER TYPE {} RENAME TO {}",
            name,
            self.quote_identifier(&stmt.to)
        )
    }

    fn add_enum_value(&self, stmt: &AddEnumValueStmt) -> String {
        let name = self.qualified_name(&stmt.enum_name, &stmt.schema);
        let position = match &stmt.position {
            EnumValuePosition::End => String::new(),
            EnumValuePosition::Before(v) => format!(" BEFORE '{}'", v),
            EnumValuePosition::After(v) => format!(" AFTER '{}'", v),
        };
        format!("ALTER TYPE {} ADD VALUE '{}'{}", name, stmt.value, position)
    }

    fn alter_enum_description(&self, stmt: &AlterEnumDescriptionStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        match &stmt.description {
            Some(desc) => {
                let escaped = desc.replace('\'', "''");
                format!("COMMENT ON TYPE {} IS '{}'", name, escaped)
            }
            None => format!("COMMENT ON TYPE {} IS NULL", name),
        }
    }

    // Sequence operations

    fn create_sequence(&self, stmt: &CreateSequenceStmt) -> String {
        let seq = &stmt.sequence;
        let name = self.qualified_name(&seq.name, &seq.schema);
        let mut parts = vec![format!("CREATE SEQUENCE {}", name)];

        parts.push(format!("INCREMENT BY {}", seq.increment));
        parts.push(format!("MINVALUE {}", seq.min_value));

        if let Some(max) = seq.max_value {
            parts.push(format!("MAXVALUE {}", max));
        } else {
            parts.push("NO MAXVALUE".to_string());
        }

        parts.push(format!("START WITH {}", seq.start));
        parts.push(format!("CACHE {}", seq.cache));

        if seq.cycle {
            parts.push("CYCLE".to_string());
        } else {
            parts.push("NO CYCLE".to_string());
        }

        parts.join(" ")
    }

    fn drop_sequence(&self, stmt: &DropSequenceStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        format!("DROP SEQUENCE {}", name)
    }

    fn alter_sequence(&self, stmt: &AlterSequenceStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        let mut parts = vec![format!("ALTER SEQUENCE {}", name)];

        for change in &stmt.changes {
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

    fn create_table(&self, stmt: &CreateTableStmt) -> Vec<String> {
        let table = &stmt.table;
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

    fn drop_table(&self, stmt: &DropTableStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        let cascade = if stmt.cascade { " CASCADE" } else { "" };
        format!("DROP TABLE {}{}", name, cascade)
    }

    fn rename_table(&self, stmt: &RenameTableStmt) -> String {
        let name = self.qualified_name(&stmt.from, &stmt.schema);
        format!(
            "ALTER TABLE {} RENAME TO {}",
            name,
            self.quote_identifier(&stmt.to)
        )
    }

    fn alter_table_comment(&self, stmt: &AlterTableCommentStmt) -> String {
        let name = self.qualified_name(&stmt.table, &stmt.schema);
        match &stmt.comment {
            Some(comment) => {
                let escaped = comment.replace('\'', "''");
                format!("COMMENT ON TABLE {} IS '{}'", name, escaped)
            }
            None => format!("COMMENT ON TABLE {} IS NULL", name),
        }
    }

    // Column operations

    fn add_column(&self, stmt: &AddColumnStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        format!(
            "ALTER TABLE {} ADD COLUMN {}",
            table,
            self.column_definition(&stmt.column)
        )
    }

    fn drop_column(&self, stmt: &DropColumnStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        let cascade = if stmt.cascade { " CASCADE" } else { "" };
        format!(
            "ALTER TABLE {} DROP COLUMN {}{}",
            table,
            self.quote_identifier(&stmt.column),
            cascade
        )
    }

    fn rename_column(&self, stmt: &RenameColumnStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            table,
            self.quote_identifier(&stmt.from),
            self.quote_identifier(&stmt.to)
        )
    }

    fn alter_column(&self, stmt: &AlterColumnStmt) -> Vec<String> {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        let column = self.quote_identifier(&stmt.column);

        stmt.changes
            .iter()
            .map(|change| match change {
                ColumnChange::SetType(t) => {
                    format!("ALTER TABLE {} ALTER COLUMN {} TYPE {}", table, column, t)
                }
                ColumnChange::SetNotNull => {
                    format!("ALTER TABLE {} ALTER COLUMN {} SET NOT NULL", table, column)
                }
                ColumnChange::DropNotNull => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                        table, column
                    )
                }
                ColumnChange::SetDefault(d) => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                        table, column, d
                    )
                }
                ColumnChange::DropDefault => {
                    format!("ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT", table, column)
                }
                ColumnChange::SetGenerated(g) => {
                    format!("ALTER TABLE {} ALTER COLUMN {} SET {}", table, column, g)
                }
                ColumnChange::DropGenerated => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP EXPRESSION",
                        table, column
                    )
                }
            })
            .collect()
    }

    fn alter_column_comment(&self, stmt: &AlterColumnCommentStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        let column = self.quote_identifier(&stmt.column);
        match &stmt.comment {
            Some(comment) => {
                let escaped = comment.replace('\'', "''");
                format!("COMMENT ON COLUMN {}.{} IS '{}'", table, column, escaped)
            }
            None => format!("COMMENT ON COLUMN {}.{} IS NULL", table, column),
        }
    }

    // Index operations

    fn create_index(&self, stmt: &CreateIndexStmt) -> String {
        let idx = &stmt.index;
        let mut sql = String::from("CREATE ");

        if idx.unique {
            sql.push_str("UNIQUE ");
        }

        sql.push_str("INDEX ");

        if stmt.concurrently {
            sql.push_str("CONCURRENTLY ");
        }

        if stmt.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }

        sql.push_str(&self.quote_identifier(&idx.name));
        sql.push_str(" ON ");
        sql.push_str(&self.qualified_name(&stmt.table, &stmt.schema));

        if idx.method != "btree" {
            sql.push_str(&format!(" USING {}", idx.method));
        }

        let cols: Vec<String> = idx
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

        if !idx.include.is_empty() {
            let include_cols: Vec<String> = idx
                .include
                .iter()
                .map(|c| self.quote_identifier(c))
                .collect();
            sql.push_str(&format!(" INCLUDE ({})", include_cols.join(", ")));
        }

        if let Some(where_clause) = &idx.where_clause {
            sql.push_str(&format!(" WHERE {}", where_clause));
        }

        sql
    }

    fn drop_index(&self, stmt: &DropIndexStmt) -> String {
        let mut sql = String::from("DROP INDEX ");

        if stmt.concurrently {
            sql.push_str("CONCURRENTLY ");
        }

        if stmt.if_exists {
            sql.push_str("IF EXISTS ");
        }

        sql.push_str(&self.qualified_name(&stmt.name, &stmt.schema));
        sql
    }

    // Constraint operations

    fn add_constraint(&self, stmt: &AddConstraintStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        format!(
            "ALTER TABLE {} ADD {}",
            table,
            self.constraint_definition(&stmt.constraint)
        )
    }

    fn drop_constraint(&self, stmt: &DropConstraintStmt) -> String {
        let table = self.qualified_name(&stmt.table, &stmt.schema);
        let cascade = if stmt.cascade { " CASCADE" } else { "" };
        format!(
            "ALTER TABLE {} DROP CONSTRAINT {}{}",
            table,
            self.quote_identifier(&stmt.name),
            cascade
        )
    }

    // View operations

    fn create_view(&self, stmt: &CreateViewStmt) -> String {
        let view = &stmt.view;
        let name = self.qualified_name(&view.name, &view.schema);

        let mut sql = String::from("CREATE ");

        if stmt.or_replace {
            sql.push_str("OR REPLACE ");
        }

        if view.materialized {
            sql.push_str("MATERIALIZED ");
        }

        sql.push_str(&format!("VIEW {} AS {}", name, view.definition));
        sql
    }

    fn drop_view(&self, stmt: &DropViewStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        let materialized = if stmt.materialized {
            "MATERIALIZED "
        } else {
            ""
        };
        let cascade = if stmt.cascade { " CASCADE" } else { "" };
        format!("DROP {}VIEW {}{}", materialized, name, cascade)
    }

    fn alter_view(&self, stmt: &AlterViewStmt) -> String {
        let name = self.qualified_name(&stmt.name, &stmt.schema);
        format!("CREATE OR REPLACE VIEW {} AS {}", name, stmt.new_definition)
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
