use colored::Colorize;

/// A single file the writer would produce, used to render previews that mirror
/// the on-disk layout without writing anything.
pub struct PreviewFile {
    /// Path relative to the output directory (e.g. `user.rs` or `user/user.rs`).
    pub path: String,
    /// Full file contents, exactly as it would be written to disk.
    pub content: String,
}

impl PreviewFile {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

/// Syntax-highlight `content` for the given `bat` language token, falling back
/// to the unhighlighted text if highlighting is unavailable.
fn highlight(content: &str, language: &str) -> String {
    let mut buffer = String::new();
    let res = bat::PrettyPrinter::new()
        .input_from_bytes(content.as_bytes())
        .language(language)
        .print_with_writer(Some(&mut buffer));

    match res {
        Ok(true) => buffer,
        _ => content.to_string(),
    }
}

/// Render a set of generated files as a terminal preview, mirroring the
/// declarative directory-schema preview: a header summarising the file count,
/// then each file under a dimmed path comment with its contents highlighted.
///
/// When `no_color` is set, paths are plain `// path` comments and contents are
/// emitted verbatim (no ANSI escapes), which keeps the output stable for tests
/// and non-TTY consumers.
pub fn render_preview(files: &[PreviewFile], language: &str, no_color: bool) -> String {
    let count = files.len().to_string();
    let count = if no_color {
        count
    } else {
        count.cyan().to_string()
    };

    let mut output = format!("{} file(s):\n", count);

    for file in files {
        let (label, content) = if no_color {
            (file.path.to_string(), file.content.clone())
        } else {
            (
                format!("{}", file.path.as_str().dimmed()),
                highlight(&file.content, language),
            )
        };
        output.push_str(&format!("\n{}\n{}", label, content));
    }

    output
}
