/// Identifier for a table, combining name and optional schema
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableId {
    pub name: String,
    pub schema: Option<String>,
}

impl From<(String, Option<String>)> for TableId {
    fn from((name, schema): (String, Option<String>)) -> Self {
        Self { name, schema }
    }
}

impl From<&(String, Option<String>)> for TableId {
    fn from((name, schema): &(String, Option<String>)) -> Self {
        Self {
            name: name.clone(),
            schema: schema.clone(),
        }
    }
}

impl TableId {
    pub fn new(name: impl Into<String>, schema: Option<String>) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }

    fn with_name(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: self.schema.clone(),
        }
    }

    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn column(&self, name: impl Into<String>) -> ColumnId {
        ColumnId::new(self.clone(), name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnId {
    table: TableId,
    name: String,
}

impl ColumnId {
    pub fn new(table: TableId, name: impl Into<String>) -> Self {
        Self {
            table,
            name: name.into(),
        }
    }

    pub fn table(&self) -> &TableId {
        &self.table
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
