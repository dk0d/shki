//! Index definitions

use serde::{Deserialize, Serialize};

/// An index on a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    /// Index name
    pub name: String,

    /// Columns or expressions in the index
    pub columns: Vec<IndexColumn>,

    /// Whether the index is unique
    #[serde(default)]
    pub unique: bool,

    /// Index method (btree, hash, gist, gin, etc.)
    #[serde(default = "default_btree")]
    pub method: IndexMethod,

    /// WHERE clause for partial index
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,

    /// Index options (e.g., fillfactor)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,

    /// Whether index is for a constraint (internal use)
    #[serde(default)]
    pub is_constraint: bool,

    /// Concurrently create the index
    #[serde(default)]
    pub concurrently: bool,

    /// Include columns (PostgreSQL covering indexes)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,

    /// Tablespace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tablespace: Option<String>,
}

fn default_btree() -> IndexMethod {
    IndexMethod::BTree
}

/// Index method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IndexMethod {
    #[default]
    BTree,
    Hash,
    Gist,
    SpGist,
    Gin,
    Brin,
}

impl IndexMethod {
    pub fn to_sql(&self) -> &'static str {
        match self {
            IndexMethod::BTree => "btree",
            IndexMethod::Hash => "hash",
            IndexMethod::Gist => "gist",
            IndexMethod::SpGist => "spgist",
            IndexMethod::Gin => "gin",
            IndexMethod::Brin => "brin",
        }
    }
}

impl std::fmt::Display for IndexMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sql())
    }
}

/// A column or expression in an index
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IndexColumn {
    /// Simple column reference
    Column {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order: Option<SortOrder>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nulls: Option<NullsOrder>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opclass: Option<String>,
    },
    /// Expression index
    Expression {
        expression: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order: Option<SortOrder>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nulls: Option<NullsOrder>,
    },
}

impl IndexColumn {
    /// Create a simple column reference
    pub fn column(name: impl Into<String>) -> Self {
        IndexColumn::Column {
            name: name.into(),
            order: None,
            nulls: None,
            opclass: None,
        }
    }

    /// Create an expression index column
    pub fn expression(expr: impl Into<String>) -> Self {
        IndexColumn::Expression {
            expression: expr.into(),
            order: None,
            nulls: None,
        }
    }

    /// Set sort order to ascending
    pub fn asc(mut self) -> Self {
        match &mut self {
            IndexColumn::Column { order, .. } => *order = Some(SortOrder::Asc),
            IndexColumn::Expression { order, .. } => *order = Some(SortOrder::Asc),
        }
        self
    }

    /// Set sort order to descending
    pub fn desc(mut self) -> Self {
        match &mut self {
            IndexColumn::Column { order, .. } => *order = Some(SortOrder::Desc),
            IndexColumn::Expression { order, .. } => *order = Some(SortOrder::Desc),
        }
        self
    }

    /// Set NULLS FIRST
    pub fn nulls_first(mut self) -> Self {
        match &mut self {
            IndexColumn::Column { nulls, .. } => *nulls = Some(NullsOrder::First),
            IndexColumn::Expression { nulls, .. } => *nulls = Some(NullsOrder::First),
        }
        self
    }

    /// Set NULLS LAST
    pub fn nulls_last(mut self) -> Self {
        match &mut self {
            IndexColumn::Column { nulls, .. } => *nulls = Some(NullsOrder::Last),
            IndexColumn::Expression { nulls, .. } => *nulls = Some(NullsOrder::Last),
        }
        self
    }

    /// Set operator class
    pub fn opclass(mut self, opclass: impl Into<String>) -> Self {
        if let IndexColumn::Column {
            opclass: ref mut op,
            ..
        } = self
        {
            *op = Some(opclass.into());
        }
        self
    }
}

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl SortOrder {
    pub fn to_sql(&self) -> &'static str {
        match self {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
}

/// Nulls ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum NullsOrder {
    First,
    Last,
}

impl NullsOrder {
    pub fn to_sql(&self) -> &'static str {
        match self {
            NullsOrder::First => "NULLS FIRST",
            NullsOrder::Last => "NULLS LAST",
        }
    }
}

impl Index {
    /// Create a new index
    pub fn new(name: impl Into<String>, columns: Vec<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            columns: columns
                .into_iter()
                .map(|c| IndexColumn::column(c))
                .collect(),
            unique: false,
            method: IndexMethod::BTree,
            where_clause: None,
            options: Vec::new(),
            is_constraint: false,
            concurrently: false,
            include: Vec::new(),
            tablespace: None,
        }
    }

    /// Create a new index with index columns
    pub fn with_columns(name: impl Into<String>, columns: Vec<IndexColumn>) -> Self {
        Self {
            name: name.into(),
            columns,
            unique: false,
            method: IndexMethod::BTree,
            where_clause: None,
            options: Vec::new(),
            is_constraint: false,
            concurrently: false,
            include: Vec::new(),
            tablespace: None,
        }
    }

    /// Make the index unique
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Set the index method
    pub fn using(mut self, method: IndexMethod) -> Self {
        self.method = method;
        self
    }

    /// Add a WHERE clause for partial index
    pub fn where_clause(mut self, clause: impl Into<String>) -> Self {
        self.where_clause = Some(clause.into());
        self
    }

    /// Create concurrently
    pub fn concurrently(mut self) -> Self {
        self.concurrently = true;
        self
    }

    /// Add include columns (covering index)
    pub fn include(mut self, columns: Vec<impl Into<String>>) -> Self {
        self.include = columns.into_iter().map(Into::into).collect();
        self
    }

    /// Set tablespace
    pub fn tablespace(mut self, tablespace: impl Into<String>) -> Self {
        self.tablespace = Some(tablespace.into());
        self
    }

    /// Add an option
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.push((key.into(), value.into()));
        self
    }
}

/// Builder for creating indexes
#[derive(Clone)]
pub struct IndexBuilder {
    index: Index,
}

impl IndexBuilder {
    /// Create a new index builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            index: Index {
                name: name.into(),
                columns: Vec::new(),
                unique: false,
                method: IndexMethod::BTree,
                where_clause: None,
                options: Vec::new(),
                is_constraint: false,
                concurrently: false,
                include: Vec::new(),
                tablespace: None,
            },
        }
    }

    /// Add a column to the index
    pub fn column(mut self, name: impl Into<String>) -> Self {
        self.index.columns.push(IndexColumn::column(name));
        self
    }

    /// Add columns to the index
    pub fn columns(mut self, names: Vec<impl Into<String>>) -> Self {
        for name in names {
            self.index.columns.push(IndexColumn::column(name));
        }
        self
    }

    /// Add an expression to the index
    pub fn expression(mut self, expr: impl Into<String>) -> Self {
        self.index.columns.push(IndexColumn::expression(expr));
        self
    }

    /// Add a pre-built index column
    pub fn index_column(mut self, column: IndexColumn) -> Self {
        self.index.columns.push(column);
        self
    }

    /// Make unique
    pub fn unique(mut self) -> Self {
        self.index.unique = true;
        self
    }

    /// Set method
    pub fn using(mut self, method: IndexMethod) -> Self {
        self.index.method = method;
        self
    }

    /// Add WHERE clause
    pub fn where_clause(mut self, clause: impl Into<String>) -> Self {
        self.index.where_clause = Some(clause.into());
        self
    }

    /// Create index concurrently
    pub fn concurrently(mut self) -> Self {
        self.index.concurrently = true;
        self
    }

    /// Include columns in the index
    pub fn include(mut self, columns: Vec<impl Into<String>>) -> Self {
        self.index.include = columns.into_iter().map(Into::into).collect();
        self
    }

    /// Set index tablespace
    pub fn tablespace(mut self, tablespace: impl Into<String>) -> Self {
        self.index.tablespace = Some(tablespace.into());
        self
    }

    /// Add index option
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.index.options.push((key.into(), value.into()));
        self
    }

    /// Build the index
    pub fn build(self) -> Index {
        self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== IndexMethod Tests ====================

    #[test]
    fn test_index_method_to_sql() {
        assert_eq!(IndexMethod::BTree.to_sql(), "btree");
        assert_eq!(IndexMethod::Hash.to_sql(), "hash");
        assert_eq!(IndexMethod::Gist.to_sql(), "gist");
        assert_eq!(IndexMethod::SpGist.to_sql(), "spgist");
        assert_eq!(IndexMethod::Gin.to_sql(), "gin");
        assert_eq!(IndexMethod::Brin.to_sql(), "brin");
    }

    #[test]
    fn test_index_method_display() {
        assert_eq!(format!("{}", IndexMethod::BTree), "btree");
        assert_eq!(format!("{}", IndexMethod::Gin), "gin");
    }

    #[test]
    fn test_index_method_default() {
        assert_eq!(IndexMethod::default(), IndexMethod::BTree);
    }

    // ==================== SortOrder Tests ====================

    #[test]
    fn test_sort_order_to_sql() {
        assert_eq!(SortOrder::Asc.to_sql(), "ASC");
        assert_eq!(SortOrder::Desc.to_sql(), "DESC");
    }

    #[test]
    fn test_sort_order_default() {
        assert_eq!(SortOrder::default(), SortOrder::Asc);
    }

    // ==================== NullsOrder Tests ====================

    #[test]
    fn test_nulls_order_to_sql() {
        assert_eq!(NullsOrder::First.to_sql(), "NULLS FIRST");
        assert_eq!(NullsOrder::Last.to_sql(), "NULLS LAST");
    }

    // ==================== IndexColumn Tests ====================

    #[test]
    fn test_index_column_simple() {
        let col = IndexColumn::column("email");
        if let IndexColumn::Column {
            name,
            order,
            nulls,
            opclass,
        } = col
        {
            assert_eq!(name, "email");
            assert!(order.is_none());
            assert!(nulls.is_none());
            assert!(opclass.is_none());
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_expression() {
        let col = IndexColumn::expression("lower(email)");
        if let IndexColumn::Expression {
            expression,
            order,
            nulls,
        } = col
        {
            assert_eq!(expression, "lower(email)");
            assert!(order.is_none());
            assert!(nulls.is_none());
        } else {
            panic!("Expected Expression variant");
        }
    }

    #[test]
    fn test_index_column_asc() {
        let col = IndexColumn::column("name").asc();
        if let IndexColumn::Column { order, .. } = col {
            assert_eq!(order, Some(SortOrder::Asc));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_desc() {
        let col = IndexColumn::column("created_at").desc();
        if let IndexColumn::Column { order, .. } = col {
            assert_eq!(order, Some(SortOrder::Desc));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_nulls_first() {
        let col = IndexColumn::column("name").nulls_first();
        if let IndexColumn::Column { nulls, .. } = col {
            assert_eq!(nulls, Some(NullsOrder::First));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_nulls_last() {
        let col = IndexColumn::column("name").nulls_last();
        if let IndexColumn::Column { nulls, .. } = col {
            assert_eq!(nulls, Some(NullsOrder::Last));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_opclass() {
        let col = IndexColumn::column("content").opclass("gin_trgm_ops");
        if let IndexColumn::Column { opclass, .. } = col {
            assert_eq!(opclass, Some("gin_trgm_ops".to_string()));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_column_chained() {
        let col = IndexColumn::column("created_at").desc().nulls_first();

        if let IndexColumn::Column { order, nulls, .. } = col {
            assert_eq!(order, Some(SortOrder::Desc));
            assert_eq!(nulls, Some(NullsOrder::First));
        } else {
            panic!("Expected Column variant");
        }
    }

    #[test]
    fn test_index_expression_with_order() {
        let col = IndexColumn::expression("lower(email)").asc().nulls_last();

        if let IndexColumn::Expression { order, nulls, .. } = col {
            assert_eq!(order, Some(SortOrder::Asc));
            assert_eq!(nulls, Some(NullsOrder::Last));
        } else {
            panic!("Expected Expression variant");
        }
    }

    // ==================== Index Tests ====================

    #[test]
    fn test_index_new() {
        let idx = Index::new("users_email_idx", vec!["email"]);
        assert_eq!(idx.name, "users_email_idx");
        assert_eq!(idx.columns.len(), 1);
        assert!(!idx.unique);
        assert_eq!(idx.method, IndexMethod::BTree);
        assert!(idx.where_clause.is_none());
    }

    #[test]
    fn test_index_new_multiple_columns() {
        let idx = Index::new(
            "users_name_email_idx",
            vec!["first_name", "last_name", "email"],
        );
        assert_eq!(idx.columns.len(), 3);
    }

    #[test]
    fn test_index_with_columns() {
        let idx = Index::with_columns(
            "users_search_idx",
            vec![
                IndexColumn::column("name").asc(),
                IndexColumn::expression("lower(email)"),
            ],
        );
        assert_eq!(idx.columns.len(), 2);
    }

    #[test]
    fn test_index_unique() {
        let idx = Index::new("users_email_unique", vec!["email"]).unique();
        assert!(idx.unique);
    }

    #[test]
    fn test_index_using() {
        let idx = Index::new("content_search_idx", vec!["content"]).using(IndexMethod::Gin);
        assert_eq!(idx.method, IndexMethod::Gin);
    }

    #[test]
    fn test_index_where_clause() {
        let idx = Index::new("active_users_idx", vec!["email"]).where_clause("is_active = true");
        assert_eq!(idx.where_clause, Some("is_active = true".to_string()));
    }

    #[test]
    fn test_index_concurrently() {
        let idx = Index::new("users_email_idx", vec!["email"]).concurrently();
        assert!(idx.concurrently);
    }

    #[test]
    fn test_index_include() {
        let idx = Index::new("users_email_idx", vec!["email"]).include(vec!["name", "created_at"]);
        assert_eq!(idx.include, vec!["name", "created_at"]);
    }

    #[test]
    fn test_index_tablespace() {
        let idx = Index::new("users_email_idx", vec!["email"]).tablespace("fast_ssd");
        assert_eq!(idx.tablespace, Some("fast_ssd".to_string()));
    }

    #[test]
    fn test_index_option() {
        let idx = Index::new("users_email_idx", vec!["email"]).option("fillfactor", "90");
        assert_eq!(
            idx.options,
            vec![("fillfactor".to_string(), "90".to_string())]
        );
    }

    #[test]
    fn test_index_multiple_options() {
        let idx = Index::new("users_email_idx", vec!["email"])
            .option("fillfactor", "90")
            .option("deduplicate_items", "on");
        assert_eq!(idx.options.len(), 2);
    }

    #[test]
    fn test_index_complex() {
        let idx = Index::new("active_users_search", vec!["email", "name"])
            .unique()
            .using(IndexMethod::BTree)
            .where_clause("is_active = true AND deleted_at IS NULL")
            .include(vec!["created_at"])
            .option("fillfactor", "80");

        assert!(idx.unique);
        assert_eq!(idx.method, IndexMethod::BTree);
        assert!(idx.where_clause.is_some());
        assert_eq!(idx.include, vec!["created_at"]);
        assert_eq!(idx.options.len(), 1);
    }

    // ==================== IndexBuilder Tests ====================

    #[test]
    fn test_index_builder_new() {
        let idx = IndexBuilder::new("test_idx").build();
        assert_eq!(idx.name, "test_idx");
        assert!(idx.columns.is_empty());
    }

    #[test]
    fn test_index_builder_column() {
        let idx = IndexBuilder::new("test_idx").column("email").build();
        assert_eq!(idx.columns.len(), 1);
    }

    #[test]
    fn test_index_builder_columns() {
        let idx = IndexBuilder::new("test_idx")
            .columns(vec!["first_name", "last_name"])
            .build();
        assert_eq!(idx.columns.len(), 2);
    }

    #[test]
    fn test_index_builder_mixed_columns() {
        let idx = IndexBuilder::new("test_idx")
            .column("email")
            .columns(vec!["first_name", "last_name"])
            .column("created_at")
            .build();
        assert_eq!(idx.columns.len(), 4);
    }

    #[test]
    fn test_index_builder_expression() {
        let idx = IndexBuilder::new("test_idx")
            .expression("lower(email)")
            .build();

        assert_eq!(idx.columns.len(), 1);
        if let IndexColumn::Expression { expression, .. } = &idx.columns[0] {
            assert_eq!(expression, "lower(email)");
        } else {
            panic!("Expected Expression variant");
        }
    }

    #[test]
    fn test_index_builder_unique() {
        let idx = IndexBuilder::new("test_idx")
            .column("email")
            .unique()
            .build();
        assert!(idx.unique);
    }

    #[test]
    fn test_index_builder_using() {
        let idx = IndexBuilder::new("test_idx")
            .column("content")
            .using(IndexMethod::Gin)
            .build();
        assert_eq!(idx.method, IndexMethod::Gin);
    }

    #[test]
    fn test_index_builder_where_clause() {
        let idx = IndexBuilder::new("test_idx")
            .column("email")
            .where_clause("is_active = true")
            .build();
        assert_eq!(idx.where_clause, Some("is_active = true".to_string()));
    }

    #[test]
    fn test_index_builder_full_example() {
        let idx = IndexBuilder::new("active_users_search")
            .column("email")
            .column("username")
            .expression("lower(name)")
            .unique()
            .using(IndexMethod::BTree)
            .where_clause("deleted_at IS NULL")
            .build();

        assert_eq!(idx.name, "active_users_search");
        assert_eq!(idx.columns.len(), 3);
        assert!(idx.unique);
        assert_eq!(idx.method, IndexMethod::BTree);
        assert!(idx.where_clause.is_some());
    }

    // ==================== Index Method Combinations ====================

    #[test]
    fn test_gin_index() {
        let idx = Index::new("posts_tags_idx", vec!["tags"]).using(IndexMethod::Gin);
        assert_eq!(idx.method, IndexMethod::Gin);
    }

    #[test]
    fn test_gist_index() {
        let idx = Index::new("locations_idx", vec!["point"]).using(IndexMethod::Gist);
        assert_eq!(idx.method, IndexMethod::Gist);
    }

    #[test]
    fn test_brin_index() {
        let idx = Index::new("events_time_idx", vec!["created_at"]).using(IndexMethod::Brin);
        assert_eq!(idx.method, IndexMethod::Brin);
    }

    #[test]
    fn test_hash_index() {
        let idx = Index::new("users_id_hash", vec!["id"]).using(IndexMethod::Hash);
        assert_eq!(idx.method, IndexMethod::Hash);
    }
}
