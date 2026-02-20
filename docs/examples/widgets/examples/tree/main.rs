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
}
