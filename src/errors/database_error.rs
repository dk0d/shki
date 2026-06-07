use sqlx::postgres::{PgDatabaseError, PgErrorPosition};

use super::diagnostic::{parse_pg_original_position_from_debug, render_source_diagnostic};

#[derive(Debug)]
pub struct DatabaseError {
    pub error: sqlx::Error,
    pub summary: Option<String>,
    pub source: Option<DiagnosticSource>,
}

#[derive(Debug)]
pub struct DiagnosticSource {
    pub name: String,
    pub content: String,
}

impl DatabaseError {
    pub fn new(error: sqlx::Error) -> Self {
        Self {
            error,
            summary: None,
            source: None,
        }
    }

    pub fn with_source(
        error: sqlx::Error,
        summary: impl Into<String>,
        source_name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            error,
            summary: Some(summary.into()),
            source: Some(DiagnosticSource {
                name: source_name.into(),
                content: source.into(),
            }),
        }
    }
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(summary) = &self.summary {
            write!(f, "{}: {}", summary, self.error)?;
        } else {
            write!(f, "{}", self.error)?;
        }

        if let Some(diagnostic) = render_database_diagnostic(&self.error, self.source.as_ref()) {
            write!(f, "\n{}", diagnostic.trim_end())?;
        }

        Ok(())
    }
}

impl std::error::Error for DatabaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

fn render_database_diagnostic(
    error: &sqlx::Error,
    source: Option<&DiagnosticSource>,
) -> Option<String> {
    let source = source?;
    let db_error = error.as_database_error()?;

    if let Some(pg_error) = db_error.try_downcast_ref::<PgDatabaseError>() {
        return match pg_error.position() {
            Some(PgErrorPosition::Original(position)) => render_source_diagnostic(
                &source.name,
                &source.content,
                position,
                pg_error.message(),
            ),
            Some(PgErrorPosition::Internal { position, query }) => {
                render_source_diagnostic("internal query", query, position, pg_error.message())
            }
            None => None,
        };
    }

    let position = parse_pg_original_position_from_debug(&format!("{db_error:?}"))?;
    render_source_diagnostic(&source.name, &source.content, position, db_error.message())
}
