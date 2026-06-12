//! Regression tests for app-level declarative binding → action dispatch (SPEC-P2).
//!
//! Tests for the `on_app_unhandled_action` fallback path and Tree widget binding
//! visibility are kept here where they can be verified through the public API.
//!
//! Notes:
//! - `active_hints_include_app_bindings_when_tree_focused` is tested internally
//!   in `src/runtime/routing.rs::message_tests` (requires `pub(crate)` API
//!   `active_binding_hints_tree`).
//! - `on_app_unhandled_action_fires_for_custom_binding` is tested internally
//!   in `src/runtime/event_loop.rs::tests` (requires private
//!   `dispatch_simulated_key_like_input`).
use textual::prelude::*;

/// All Tree navigation bindings must be hidden (show=false) so they do not
/// flood the Footer when the Tree is focused, matching Python `show=False`.
#[test]
fn tree_nav_bindings_are_hidden() {
    let tree = Tree::new(vec![TreeNode::new("r")]);
    for binding in tree.bindings() {
        assert!(
            !binding.show,
            "Tree binding {:?}/{:?} must be hidden (show=false), matching Python's show=False",
            binding.key, binding.description
        );
    }
}

/// A freshly-created TreeNode must start collapsed (expanded=false),
/// matching Python Textual where new nodes are NOT auto-expanded.
#[test]
fn tree_node_starts_collapsed() {
    let n = TreeNode::new("x");
    assert_eq!(n.is_expanded(), false, "TreeNode::new must start collapsed");
}
