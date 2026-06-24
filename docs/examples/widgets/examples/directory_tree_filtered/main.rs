/// Port of Python Textual `docs/examples/widgets/directory_tree_filtered.py`.
///
/// Demonstrates `DirectoryTree` with a custom path filter that hides dotfiles
/// (files and directories whose names start with `.`).
///
/// Python uses a `FilteredDirectoryTree` subclass that overrides `filter_paths`.
/// Rust achieves the same by passing a static predicate function to
/// `DirectoryTree::filter_paths()`.
///
/// The filter applies on every load path — the initial synchronous build *and*
/// the async lazy subdirectory load (`AsyncTaskRequest::ReadDirectory`) — matching
/// Python `DirectoryTree.filter_paths`, whose filter runs inside the single
/// `_load_directory` worker used for all loads. Expanding a nested directory keeps
/// dotfiles hidden just like the top level.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Filter predicate
// ---------------------------------------------------------------------------

/// Exclude dotfiles and hidden directories (names starting with `.`).
///
/// Mirrors Python's:
/// ```python
/// def filter_paths(self, paths):
///     return [p for p in paths if not p.name.startswith(".")]
/// ```
fn no_dotfiles(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.starts_with('.'))
        .unwrap_or(true)
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct FilteredDirectoryTreeApp;

impl TextualApp for FilteredDirectoryTreeApp {
    fn compose(&mut self) -> AppRoot {
        let mut tree = DirectoryTree::new("./");
        tree.filter_paths(no_dotfiles);
        AppRoot::new().with_child(tree)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> textual::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    textual::run_sync_snapshot(FilteredDirectoryTreeApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02 / DG-04)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_dotfiles_filter_excludes_hidden_entries() {
        let hidden_dir = std::path::Path::new("/some/dir/.git");
        let hidden_file = std::path::Path::new("/some/dir/.gitignore");
        let visible_dir = std::path::Path::new("/some/dir/src");
        let visible_file = std::path::Path::new("/some/dir/Cargo.toml");

        assert!(!no_dotfiles(hidden_dir), ".git should be excluded");
        assert!(!no_dotfiles(hidden_file), ".gitignore should be excluded");
        assert!(no_dotfiles(visible_dir), "src should be included");
        assert!(no_dotfiles(visible_file), "Cargo.toml should be included");
    }

    #[test]
    fn no_dotfiles_filter_includes_root_like_paths() {
        // Paths with no file_name component (e.g. bare "/") default to included.
        let root = std::path::Path::new("/");
        assert!(no_dotfiles(root), "bare root should default to included");
    }

    #[test]
    fn filtered_directory_tree_app_composes_without_panic() {
        let mut app = FilteredDirectoryTreeApp;
        let _root = app.compose();
        // compose() must not panic — tree is created with filter set.
    }
}
