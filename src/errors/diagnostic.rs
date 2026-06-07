use ariadne::{Color, Config as AriadneConfig, Label, Report, ReportKind, Source};

pub fn render_source_diagnostic(
    source_name: &str,
    source: &str,
    position: usize,
    message: &str,
) -> Option<String> {
    let byte_offset = char_position_to_byte_offset(source, position)?;
    let span = byte_offset..byte_offset.saturating_add(1).min(source.len());
    let mut output = Vec::new();

    Report::build(ReportKind::Error, (source_name, span.clone()))
        .with_config(AriadneConfig::default().with_compact(true))
        .with_message("source failed validation")
        .with_label(
            Label::new((source_name, span))
                .with_message(message)
                .with_color(Color::Red),
        )
        .finish()
        .write((source_name, Source::from(source)), &mut output)
        .ok()?;

    String::from_utf8(output).ok()
}

pub fn char_position_to_byte_offset(source: &str, position: usize) -> Option<usize> {
    let target = position.saturating_sub(1);
    source
        .char_indices()
        .nth(target)
        .map(|(offset, _)| offset)
        .or_else(|| (target == source.chars().count()).then_some(source.len()))
}

pub fn parse_pg_original_position_from_debug(debug: &str) -> Option<usize> {
    let position = debug.split("position: Some(Original(").nth(1)?;
    let position = position.split(')').next()?;
    position.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_source_diagnostic_uses_ariadne_report() {
        let source =
            "ALTER TYPE status ADD VALUE 'archived';\nCREATE TABLE status_events (id integer);";
        let position = source
            .find("status_events")
            .expect("test SQL should contain table")
            + 1;
        let diagnostic = render_source_diagnostic(
            "generated migration SQL",
            source,
            position,
            "bad enum value",
        )
        .expect("position should render");

        assert!(diagnostic.contains("source failed validation"));
        assert!(diagnostic.contains("bad enum value"));
    }

    #[test]
    fn parse_pg_original_position_from_debug_extracts_position() {
        let debug = "PgDatabaseError { message: \"bad\", position: Some(Original(42)) }";

        assert_eq!(parse_pg_original_position_from_debug(debug), Some(42));
    }
}
