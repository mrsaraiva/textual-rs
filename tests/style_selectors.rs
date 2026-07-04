use rich_rs::Console;
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame_with_stylesheet};

fn render_with_sheet(
    root: &mut dyn Widget,
    width: usize,
    height: usize,
    stylesheet: StyleSheet,
) -> textual::render::FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should exist");
    render_tree_to_frame_with_stylesheet(&mut tree, root, &console, width, height, stylesheet)
}

#[test]
fn descendant_selectors_match() {
    let css = ".panel Label { underline: true; }";
    let sheet = StyleSheet::parse(css);

    // A `.class("panel")` on the ROOT Container must land on the root node so its
    // children match `.panel Label`. Regression guard for the seed-based root
    // identity harvest in `build_widget_tree_from_root` (the Node deletion made
    // `.class()` seed-based; the root propagation used to read the Node wrapper's
    // `style_classes()` and silently dropped a seed-based root's classes).
    let mut row = Container::new()
        .with_child(Label::new("hi"))
        .with_child(Label::new("x"))
        .class("panel");
    let buf = render_with_sheet(&mut row, 8, 2, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}

#[test]
fn child_selectors_match_direct_children_only() {
    let css = ".panel > Label { bold: true; }";
    let sheet = StyleSheet::parse(css);

    // Direct-child case: the `.panel` ROOT Container is the immediate parent of the
    // `Label`. Same seed-based root-identity regression guard as
    // `descendant_selectors_match`, for the `>` (direct child) combinator.
    let mut row = Container::new()
        .with_child(Label::new("hi"))
        .with_child(Label::new("x"))
        .class("panel");
    let buf = render_with_sheet(&mut row, 8, 2, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn root_widget_seed_identity_lands_on_root_node() {
    // Tightest guard on the exact bug: a seed-based widget used as the tree ROOT
    // must keep its `.id()` + `.class()` on the root arena node (previously the
    // seed was never consumed for the root, so identity was silently dropped).
    let mut root = Container::new()
        .with_child(Label::new("hi"))
        .id("hero")
        .class("panel")
        .class("boxed");
    let tree = build_widget_tree_from_root(&mut root).expect("tree should exist");
    let root_id = tree.root().expect("root node");

    assert_eq!(tree.css_id(root_id), Some("hero"));
    assert!(tree.has_class(root_id, "panel"));
    assert!(tree.has_class(root_id, "boxed"));
}

#[test]
fn selector_groups_apply_to_multiple() {
    let css = "Label, .note { bold: true; }";
    let sheet = StyleSheet::parse(css);

    let mut label = Container::new().with_child(Label::new("hi").class("note"));
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn selector_with_multiple_classes_matches() {
    let css = ".primary.big { underline: true; }";
    let sheet = StyleSheet::parse(css);

    let mut label = Container::new().with_child(Label::new("hi").class("primary").class("big"));
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}
