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

    let mut row = Node::new(Row::new().with_child(Label::new("hi"))).class("panel");
    let buf = render_with_sheet(&mut row, 8, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}

#[test]
fn child_selectors_match_direct_children_only() {
    let css = ".panel > Label { bold: true; }";
    let sheet = StyleSheet::parse(css);

    // Direct child case: the `.panel` node is the immediate parent of the `Label`.
    let mut row = Node::new(Label::new("hi")).class("panel");
    let buf = render_with_sheet(&mut row, 8, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn selector_groups_apply_to_multiple() {
    let css = "Label, .note { bold: true; }";
    let sheet = StyleSheet::parse(css);

    let mut label = Node::new(Label::new("hi")).class("note");
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn selector_with_multiple_classes_matches() {
    let css = ".primary.big { underline: true; }";
    let sheet = StyleSheet::parse(css);

    let mut label = Node::new(Label::new("hi")).class("primary").class("big");
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}
