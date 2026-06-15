/// Port of Python Textual `docs/examples/widgets/directory_tree.py`.
///
/// Demonstrates the `DirectoryTree` widget:
/// - Shows the current directory (`"./"`).
/// - No custom CSS or bindings — the widget stands alone.
use textual::prelude::*;

struct DirectoryTreeApp;

impl TextualApp for DirectoryTreeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(DirectoryTree::new("./"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(DirectoryTreeApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_tree_app_composes_without_panic() {
        let mut app = DirectoryTreeApp;
        let _root = app.compose();
    }
}
