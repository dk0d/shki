use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{Result, ShkiError};

pub const DIRECTORY_SCHEMA_ENTRYPOINT: &str = "main.sql";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeSchema {
    pub entrypoint: PathBuf,
    pub sql: String,
}

pub fn load_declarative_schema(path: impl AsRef<Path>) -> Result<DeclarativeSchema> {
    let path = path.as_ref();
    let entrypoint = if path.is_dir() {
        path.join(DIRECTORY_SCHEMA_ENTRYPOINT)
    } else {
        path.to_path_buf()
    };

    if !entrypoint.exists() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint does not exist: {}",
            entrypoint.display()
        )));
    }

    if !entrypoint.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint is not a file: {}",
            entrypoint.display()
        )));
    }

    let mut loading = Vec::new();
    let mut loaded = HashSet::new();
    let sql = load_sql_file(&entrypoint, &mut loading, &mut loaded)?;

    Ok(DeclarativeSchema { entrypoint, sql })
}

fn load_sql_file(
    path: &Path,
    loading: &mut Vec<PathBuf>,
    loaded: &mut HashSet<PathBuf>,
) -> Result<String> {
    let canonical = canonicalize_existing_file(path)?;

    if let Some(index) = loading.iter().position(|active| active == &canonical) {
        let mut cycle = loading[index..]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        cycle.push(canonical.display().to_string());
        return Err(ShkiError::schema(format!(
            "Cyclic Declarative Schema include detected: {}",
            cycle.join(" -> ")
        )));
    }

    if loaded.contains(&canonical) {
        return Ok(String::new());
    }

    loading.push(canonical.clone());
    let content = std::fs::read_to_string(&canonical)?;
    let mut expanded = String::new();

    for line in content.lines() {
        if let Some(include_path) = parse_include_directive(line)? {
            let include_path = canonical
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(include_path);
            expanded.push_str(&load_sql_file(&include_path, loading, loaded)?);
            if !expanded.ends_with('\n') {
                expanded.push('\n');
            }
        } else {
            expanded.push_str(line);
            expanded.push('\n');
        }
    }

    loading.pop();
    loaded.insert(canonical);
    Ok(expanded)
}

fn canonicalize_existing_file(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|err| {
        ShkiError::schema(format!(
            "Failed to read Declarative Schema file {}: {}",
            path.display(),
            err
        ))
    })?;

    if !canonical.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema include is not a file: {}",
            canonical.display()
        )));
    }

    Ok(canonical)
}

fn parse_include_directive(line: &str) -> Result<Option<PathBuf>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('\\') {
        return Ok(None);
    }

    let Some(rest) = trimmed.strip_prefix("\\i") else {
        return Err(unsupported_backslash_command(trimmed));
    };

    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return Err(unsupported_backslash_command(trimmed));
    }

    let rest = strip_sql_line_comment(rest.trim());
    if rest.is_empty() {
        return Err(ShkiError::schema("Declarative Schema include is missing a path"));
    }

    Ok(Some(PathBuf::from(unquote_include_path(rest)?)))
}

fn strip_sql_line_comment(value: &str) -> &str {
    value.split_once("--").map(|(value, _)| value).unwrap_or(value).trim()
}

fn unquote_include_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return Ok(value[1..value.len() - 1].to_string());
        }
    }

    if value.split_whitespace().count() > 1 {
        return Err(ShkiError::schema(format!(
            "Declarative Schema include paths with spaces must be quoted: {value}"
        )));
    }

    Ok(value.to_string())
}

fn unsupported_backslash_command(command: &str) -> ShkiError {
    let command = command.split_whitespace().next().unwrap_or(command);
    ShkiError::schema(format!(
        "Unsupported Declarative Schema command `{command}`. Only `\\i` includes are supported"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn loads_single_sql_file() {
        let temp = TempDir::new().expect("temp dir");
        let schema = temp.path().join("schema.sql");
        std::fs::write(&schema, "CREATE TABLE users (id int);\n").expect("write schema");

        let loaded = load_declarative_schema(&schema).expect("load schema");

        assert_eq!(loaded.entrypoint, schema);
        assert_eq!(loaded.sql, "CREATE TABLE users (id int);\n");
    }

    #[test]
    fn loads_directory_schema_from_main_sql() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "SELECT 1;\n").expect("write main");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.entrypoint, temp.path().join("main.sql"));
        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn expands_relative_i_includes() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::create_dir(temp.path().join("tables")).expect("create tables dir");
        std::fs::write(
            temp.path().join("main.sql"),
            "CREATE SCHEMA app;\n\\i tables/users.sql\n",
        )
        .expect("write main");
        std::fs::write(
            temp.path().join("tables/users.sql"),
            "CREATE TABLE app.users (id int);\n",
        )
        .expect("write users");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(
            loaded.sql,
            "CREATE SCHEMA app;\nCREATE TABLE app.users (id int);\n"
        );
    }

    #[test]
    fn supports_quoted_include_paths() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i 'user table.sql'\n")
            .expect("write main");
        std::fs::write(temp.path().join("user table.sql"), "SELECT 1;\n").expect("write file");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn rejects_include_cycles() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i a.sql\n").expect("write main");
        std::fs::write(temp.path().join("a.sql"), "\\i b.sql\n").expect("write a");
        std::fs::write(temp.path().join("b.sql"), "\\i a.sql\n").expect("write b");

        let error = load_declarative_schema(temp.path()).expect_err("cycle should fail");

        assert!(error.to_string().contains("Cyclic Declarative Schema include"));
    }

    #[test]
    fn rejects_unsupported_backslash_commands() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("schema.sql"), "\\ir other.sql\n").expect("write schema");

        let error = load_declarative_schema(temp.path().join("schema.sql"))
            .expect_err("unsupported command should fail");

        assert!(error.to_string().contains("Only `\\i` includes are supported"));
    }
}
