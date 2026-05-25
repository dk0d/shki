use std::path::{Path, PathBuf};

/// Resolve a path relative to the project root.
///
/// If the path is absolute, it is returned as-is.
/// If the path is relative, it is joined with the root directory.
pub fn resolve_path(root: Option<PathBuf>, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();

    let root = if let Some(root) = root {
        Some(root)
    } else {
        std::env::current_dir().ok()
    };

    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(root) = root {
        root.join(path)
    } else {
        path.to_path_buf()
    }
}
