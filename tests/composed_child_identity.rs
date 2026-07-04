//! Regression guard: a composed child's `.with_id()` / `.with_classes()`
//! (attached at the `ChildDecl` level, not baked into the widget) must reach the
//! mounted ARENA node so id/class selectors resolve.
//!
//! These cases were historically broken (the cold-app friction reports for the
//! pre-trait-split Pomodoro/Kanban apps noted composed leaf ids silently
//! dropping, forcing container wrappers and type-queries). The seed-identity /
//! `child_decl_meta` harvest now applies them end-to-end; this locks that in.

use textual::prelude::*;
use textual::runtime::build_widget_tree_from_root;

fn id_resolves(root: &mut dyn Widget, id: &str) -> bool {
    let tree = build_widget_tree_from_root(root).expect("tree");
    let root_id = tree.root().expect("root");
    tree.walk_depth_first(root_id)
        .into_iter()
        .any(|nid| tree.css_id(nid) == Some(id))
}

fn class_resolves(root: &mut dyn Widget, class: &str) -> bool {
    let tree = build_widget_tree_from_root(root).expect("tree");
    let root_id = tree.root().expect("root");
    tree.walk_depth_first(root_id)
        .into_iter()
        .any(|nid| tree.has_class(nid, class))
}

#[test]
fn composed_leaf_id_lands_inside_a_container() {
    let mut r = Container::new()
        .with_compose(vec![ChildDecl::from(Label::new("hi")).with_id("disp")]);
    assert!(id_resolves(&mut r, "disp"), "Container > ChildDecl::with_id(Label)");
}

#[test]
fn composed_leaf_classes_land_inside_a_container() {
    let mut r = Container::new()
        .with_compose(vec![ChildDecl::from(Label::new("x")).with_classes(&["tag"])]);
    assert!(class_resolves(&mut r, "tag"), "Container > ChildDecl::with_classes(Label)");
}

#[test]
fn composed_leaf_id_lands_as_direct_approot_child() {
    // The Kanban modal report said a direct screen-root child's id did not
    // resolve (forcing a VerticalGroup wrapper). It now does.
    let mut r = AppRoot::new()
        .with_compose(vec![ChildDecl::from(Label::new("hi")).with_id("hero")]);
    assert!(id_resolves(&mut r, "hero"), "AppRoot > ChildDecl::with_id(Label) DIRECT");
}

#[test]
fn composed_digits_id_resolves() {
    // The Pomodoro report's exact case: a composed Digits (no native id builder)
    // addressed via ChildDecl::with_id.
    let mut r = Container::new()
        .with_compose(vec![ChildDecl::from(Digits::new("12:00")).with_id("clock")]);
    assert!(id_resolves(&mut r, "clock"), "Container > ChildDecl::with_id(Digits)");
}
