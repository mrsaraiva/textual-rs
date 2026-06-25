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

    /// LIVENESS: the DirectoryTree auto-focuses and loads "./"; pressing `down`
    /// moves the tree cursor to the next node, re-rendering the highlight. The
    /// rendered frame must change. Proves tree navigation is wired.
    #[test]
    fn arrow_navigation_changes_frame() {
        textual::run_test(DirectoryTreeApp, |pilot| {
            // Let the initial directory load settle.
            pilot.pause()?;
            pilot.press(&["tab"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["down"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing 'down' must move the DirectoryTree cursor and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
