/// Port of Python Textual `docs/examples/widgets/tree.py`.
///
/// Demonstrates the `Tree` widget:
/// - Root node "Dune" is expanded.
/// - A "Characters" sub-node contains three leaf nodes (Paul, Jessica, Chani).
///
/// Python uses a mutable `tree.root.expand()` + `add()` / `add_leaf()` API.
/// Rust uses `TreeNode` builder pattern: `.expanded(true)`, `.with_child(...)`.
use textual::prelude::*;

struct TreeApp;

impl TextualApp for TreeApp {
    fn compose(&mut self) -> AppRoot {
        let tree = Tree::new(vec![
            TreeNode::new("Dune")
                .expanded(true)
                .allow_expand(true)
                .with_child(
                    TreeNode::new("Characters")
                        .expanded(true)
                        .allow_expand(true)
                        .with_child(TreeNode::new("Paul"))
                        .with_child(TreeNode::new("Jessica"))
                        .with_child(TreeNode::new("Chani")),
                ),
        ]);
        AppRoot::new().with_child(tree)
    }
}

fn main() -> textual::Result<()> {
    run_sync(TreeApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_app_composes_without_panic() {
        let mut app = TreeApp;
        let _root = app.compose();
    }

    #[test]
    fn tree_builds_with_nested_children() {
        let _tree = Tree::new(vec![
            TreeNode::new("Dune")
                .expanded(true)
                .allow_expand(true)
                .with_child(
                    TreeNode::new("Characters")
                        .expanded(true)
                        .allow_expand(true)
                        .with_child(TreeNode::new("Paul"))
                        .with_child(TreeNode::new("Jessica"))
                        .with_child(TreeNode::new("Chani")),
                ),
        ]);
        // Tree widget is constructed without panic.
    }

    #[test]
    fn tree_node_with_child_builder() {
        let _node = TreeNode::new("root")
            .expanded(true)
            .allow_expand(true)
            .with_child(TreeNode::new("leaf"));
        // Builder chain composes without panic.
    }

    #[test]
    fn tree_new_accepts_multiple_roots() {
        let _tree = Tree::new(vec![
            TreeNode::new("Alpha"),
            TreeNode::new("Beta"),
        ]);
        // Multi-root tree is accepted.
    }

    /// LIVENESS: focus the Tree, move the cursor down to highlight a node, then
    /// press Left to collapse the expanded root ("Dune") — its children
    /// disappear from the render, so the frame must change. A dead Tree (keys
    /// unhandled / not focusable) leaves the frame identical.
    #[test]
    fn liveness_navigate_and_collapse() {
        TreeApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?; // focus the tree
                let before = pilot.app().frame_fingerprint();
                // Highlight the root, then collapse it (Left = collapse_or_parent).
                pilot.press(&["down", "left"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "navigating/collapsing the tree must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
