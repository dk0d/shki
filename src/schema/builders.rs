use super::{
    CheckConstraint, Column, ColumnBuilder, Constraint, DataType, EnumType, ForeignKeyConstraint,
    Index, IndexBuilder, PrimaryKeyConstraint, ReferenceAction, Schema, Table, UniqueConstraint,
};

/// Builder for creating schemas with a fluent API
pub struct SchemaBuilder {
    schema: Schema,
}

impl SchemaBuilder {
    /// Create a new schema builder
    pub fn new(name: impl Into<String>, dialect: super::SchemaDialect) -> Self {
        Self {
            schema: Schema::new(name, dialect),
        }
    }

    /// Add a table using a builder closure
    pub fn table(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(TableBuilder) -> TableBuilder,
    ) -> Self {
        let builder = TableBuilder::new(name);
        let builder = f(builder);
        self.schema.table(builder.build());
        self
    }

    /// Add an enum type with values
    pub fn enum_type_values(
        mut self,
        name: impl Into<String>,
        values: Vec<impl Into<String>>,
    ) -> Self {
        self.schema.enum_type(EnumBuilder::new(name).values(values));
        self
    }

    /// Add an enum using a builder
    pub fn enum_type(mut self, enum_type: impl Into<EnumType>) -> Self {
        self.schema.enum_type(enum_type);
        self
    }

    /// Add an extension (PostgreSQL)
    pub fn extension(mut self, name: impl Into<String>) -> Self {
        self.schema.extension(name);
        self
    }

    /// Build the schema
    pub fn build(self) -> Schema {
        self.schema
    }
}

/// Builder for creating tables with a fluent API
///
/// # Example
///
/// ```rust
/// use shki::schema::{TableBuilder, ColumnBuilder};
///
/// let table = TableBuilder::new("users")
///     .column(ColumnBuilder::serial("id").primary_key())
///     .column(ColumnBuilder::text("username").not_null().unique())
///     .column(ColumnBuilder::text("email").not_null())
///     .column(ColumnBuilder::timestamptz("created_at").default_now())
///     .index("users_email_idx", vec!["email"])
///     .build();
/// ```
pub struct TableBuilder {
    table: Table,
}

impl TableBuilder {
    /// Create a new table builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            table: Table::new(name),
        }
    }

    /// Set the schema
    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.table.schema = Some(schema.into());
        self
    }

    /// Add a column from a ColumnBuilder
    pub fn column(mut self, column: impl Into<Column>) -> Self {
        self.table.column(column.into());
        self
    }

    /// Add a column using a closure
    pub fn column_fn(
        mut self,
        name: impl Into<String>,
        data_type: DataType,
        f: impl FnOnce(ColumnBuilder) -> ColumnBuilder,
    ) -> Self {
        let builder = ColumnBuilder::new(name, data_type);
        let builder = f(builder);
        self.table.column(builder.build());
        self
    }

    // ==================== Constraints ====================

    /// Add a primary key constraint
    pub fn primary_key(mut self, columns: Vec<impl Into<String>>) -> Self {
        self.table
            .constraint(Constraint::PrimaryKey(PrimaryKeyConstraint::new(columns)));
        self
    }

    /// Add a unique constraint
    pub fn unique_constraint(mut self, columns: Vec<impl Into<String>>) -> Self {
        self.table
            .constraint(Constraint::Unique(UniqueConstraint::new(columns)));
        self
    }

    /// Add a named unique constraint
    pub fn unique_constraint_named(
        mut self,
        name: impl Into<String>,
        columns: Vec<impl Into<String>>,
    ) -> Self {
        self.table.constraint(Constraint::Unique(
            UniqueConstraint::new(columns).named(name),
        ));
        self
    }

    /// Add a foreign key constraint
    pub fn foreign_key(
        mut self,
        columns: Vec<impl Into<String>>,
        references_table: impl Into<String>,
        references_columns: Vec<impl Into<String>>,
    ) -> Self {
        self.table
            .constraint(Constraint::ForeignKey(ForeignKeyConstraint::new(
                columns,
                references_table,
                references_columns,
            )));
        self
    }

    /// Add a foreign key constraint with actions
    pub fn foreign_key_with_actions(
        mut self,
        columns: Vec<impl Into<String>>,
        references_table: impl Into<String>,
        references_columns: Vec<impl Into<String>>,
        on_delete: ReferenceAction,
        on_update: ReferenceAction,
    ) -> Self {
        let fk = ForeignKeyConstraint::new(columns, references_table, references_columns)
            .on_delete(on_delete)
            .on_update(on_update);
        self.table.constraint(Constraint::ForeignKey(fk));
        self
    }

    /// Add a check constraint
    pub fn check(mut self, expression: impl Into<String>) -> Self {
        self.table
            .constraint(Constraint::Check(CheckConstraint::new(expression)));
        self
    }

    /// Add a named check constraint
    pub fn check_named(mut self, name: impl Into<String>, expression: impl Into<String>) -> Self {
        self.table.constraint(Constraint::Check(
            CheckConstraint::new(expression).named(name),
        ));
        self
    }

    // ==================== Indexes ====================

    /// Add an index
    pub fn index(mut self, name: impl Into<String>, columns: Vec<impl Into<String>>) -> Self {
        self.table.index(Index::new(name, columns));
        self
    }

    /// Add a unique index
    pub fn unique_index(
        mut self,
        name: impl Into<String>,
        columns: Vec<impl Into<String>>,
    ) -> Self {
        self.table.index(Index::new(name, columns).unique());
        self
    }

    /// Add an index using a builder
    pub fn index_builder(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(IndexBuilder) -> IndexBuilder,
    ) -> Self {
        let builder = IndexBuilder::new(name);
        let builder = f(builder);
        self.table.index(builder.build());
        self
    }

    /// Set a comment on the table
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.table.comment = Some(comment.into());
        self
    }

    /// Set a description for the table (alias for comment)
    ///
    /// The description is used for SQL comments and Rust doc comments in generated code.
    pub fn description(self, description: impl Into<String>) -> Self {
        self.comment(description)
    }

    /// Build the table
    pub fn build(self) -> Table {
        self.table
    }
}

/// Builder for creating enum types with a fluent API
///
/// # Example
///
/// ```rust
/// use shki::schema::EnumBuilder;
///
/// let status = EnumBuilder::new("post_status")
///     .value("draft")
///     .value("published")
///     .value("archived")
///     .build();
///
/// // Or with multiple values at once
/// let role = EnumBuilder::new("user_role")
///     .values(["admin", "moderator", "user", "guest"])
///     .build();
/// ```
pub struct EnumBuilder {
    enum_type: EnumType,
}

impl EnumBuilder {
    /// Create a new enum builder with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            enum_type: EnumType::new(name),
        }
    }

    /// Add a single value to the enum
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.enum_type.values.push(value.into());
        self
    }

    /// Add multiple values to the enum
    pub fn values<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.enum_type
            .values
            .extend(values.into_iter().map(Into::into));
        self
    }

    /// Set a description for the enum (used for SQL comments and Rust doc comments)
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.enum_type.description = Some(description.into());
        self
    }

    /// Build the enum type
    pub fn build(self) -> EnumType {
        self.enum_type
    }
}

/// Allow EnumBuilder to be converted to EnumType
impl From<EnumBuilder> for EnumType {
    fn from(builder: EnumBuilder) -> Self {
        builder.build()
    }
}
