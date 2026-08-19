use crate::models::iden::Iden;
use owo_colors::OwoColorize;
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RenameKind {
    Type,
    Table,
    Column,
    Index,
    Constraint,
}

impl std::fmt::Display for RenameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameKind::Type => write!(f, "type"),
            RenameKind::Table => write!(f, "table"),
            RenameKind::Column => write!(f, "column"),
            RenameKind::Index => write!(f, "index"),
            RenameKind::Constraint => write!(f, "constraint"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenameId {
    pub kind: RenameKind,
    pub table: Iden,
    pub name: String,
}

impl std::fmt::Display for RenameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({}) {}",
            format!("{}", self.kind).dimmed(),
            self.name.bold()
        )
    }
}

impl RenameId {
    pub fn table(table: Iden) -> Self {
        Self {
            kind: RenameKind::Table,
            table: table.clone(),
            name: table.name,
        }
    }

    pub fn type_(name: Iden) -> Self {
        Self {
            kind: RenameKind::Type,
            table: name.clone(),
            name: name.name,
        }
    }

    pub fn column(table: Iden, name: impl Into<String>) -> Self {
        Self::table_object(RenameKind::Column, table, name)
    }

    pub fn index(table: Iden, name: impl Into<String>) -> Self {
        Self::table_object(RenameKind::Index, table, name)
    }

    pub fn constraint(table: Iden, name: impl Into<String>) -> Self {
        Self::table_object(RenameKind::Constraint, table, name)
    }

    fn table_object(kind: RenameKind, table: Iden, name: impl Into<String>) -> Self {
        Self {
            kind,
            table,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RenameSelection {
    Create(RenameId),
    Rename { name: RenameId, new_name: RenameId },
}

impl std::fmt::Display for RenameSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameSelection::Create(name) => {
                write!(f, "{} {} {}", "+".green(), "Create".bold().green(), name)
            }
            RenameSelection::Rename { name, new_name } => {
                write!(
                    f,
                    "{} {} {} -> {}",
                    "~".yellow(),
                    "Rename".yellow(),
                    name,
                    new_name
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenameMap {
    pub source: RenameId,
    pub target: RenameId,
}

impl RenameMap {
    pub fn type_(source: Iden, target: Iden) -> Self {
        Self::new(RenameId::type_(source), RenameId::type_(target))
    }

    pub fn table(source: Iden, target: Iden) -> Self {
        Self::new(RenameId::table(source), RenameId::table(target))
    }

    pub fn column(table: Iden, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::new(
            RenameId::column(table.clone(), from),
            RenameId::column(table, to),
        )
    }

    pub fn index(table: Iden, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::new(
            RenameId::index(table.clone(), from),
            RenameId::index(table, to),
        )
    }

    pub fn constraint(table: Iden, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::new(
            RenameId::constraint(table.clone(), from),
            RenameId::constraint(table, to),
        )
    }

    pub fn new(source: RenameId, target: RenameId) -> Self {
        Self { source, target }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RenameDecision {
    Rename(RenameMap),
    Drop(RenameId),
}

#[derive(Debug, Clone)]
pub struct RenameScenario {
    pub kind: RenameKind,
    pub table: Option<Iden>,
    pub created: IndexMap<String, RenameId>,
    pub dropped: IndexMap<String, RenameId>,
}
