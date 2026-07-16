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

/// Port of Python `test_dynamic_bindings.py::test_dynamic_disabled`: a key
/// binding whose action is dynamically disabled via `check_action` must NOT
/// run when the key is pressed. `Some(false)` (hidden) and `None` (disabled,
/// shown dimmed) both suppress the action; only `Some(true)` lets it fire.
#[test]
fn dynamic_disabled_binding_does_not_fire_on_keypress() {
    use std::sync::{Arc, Mutex};

    struct DynamicApp {
        actions: Arc<Mutex<Vec<String>>>,
    }

    impl TextualApp for DynamicApp {
        fn bindings(&self) -> Vec<BindingDecl> {
            vec![
                BindingDecl::new("a", "register('a')", "A"),
                BindingDecl::new("b", "register('b')", "B"),
                BindingDecl::new("c", "register('c')", "C"),
            ]
        }

        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Label::new("dynamic bindings"))
        }

        fn check_action(&self, action: &str, parameters: &[String]) -> Option<bool> {
            if action == "register" {
                if parameters == ["b"] {
                    return Some(false);
                }
                if parameters == ["c"] {
                    return None;
                }
            }
            Some(true)
        }

        fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
            if action.starts_with("register") {
                self.actions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(action.to_string());
                ctx.set_handled();
            }
        }
    }

    let actions: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&actions);
    run_test(DynamicApp { actions }, |pilot| {
        pilot.press(&["a", "b", "c"])?;
        Ok(())
    })
    .expect("run_test");

    assert_eq!(
        *recorded.lock().unwrap_or_else(|e| e.into_inner()),
        vec!["register('a')".to_string()],
        "only the enabled binding ('a') may run; 'b' (Some(false)) and 'c' (None) are gated off"
    );
}

/// Port of Python `test_keys.py::test_character_bindings`: you can bind to a
/// raw punctuation character (`"."`, `"~"`) as well as a long key name
/// (`"space"`); pressing the character fires the binding, and an unbound key
/// does not.
#[test]
fn character_bindings_fire_on_keypress() {
    use std::sync::{Arc, Mutex};

    struct BindApp {
        counter: Arc<Mutex<u32>>,
    }

    impl TextualApp for BindApp {
        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new(".,~,space", "increment", "foo")]
        }

        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Label::new("character bindings"))
        }

        fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
            if action == "increment" {
                *self.counter.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                ctx.set_handled();
            }
        }
    }

    let counter: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&counter);
    run_test(BindApp { counter }, |pilot| {
        pilot.press(&["."])?;
        assert_eq!(
            *observed.lock().unwrap_or_else(|e| e.into_inner()),
            1,
            "'.' must fire the '.' binding alternative"
        );
        pilot.press(&["~"])?;
        assert_eq!(
            *observed.lock().unwrap_or_else(|e| e.into_inner()),
            2,
            "'~' must fire the '~' binding alternative"
        );
        pilot.press(&["space"])?;
        assert_eq!(
            *observed.lock().unwrap_or_else(|e| e.into_inner()),
            3,
            "space must fire the 'space' binding alternative"
        );
        pilot.press(&["x"])?;
        assert_eq!(
            *observed.lock().unwrap_or_else(|e| e.into_inner()),
            3,
            "an unbound key must not fire the binding"
        );
        Ok(())
    })
    .expect("run_test");
}
