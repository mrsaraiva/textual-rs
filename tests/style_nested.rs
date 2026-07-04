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
fn nested_descendant_selector_applies() {
    let css = r#"
    Row {
        Label { underline: true; }
    }
    "#;
    let sheet = StyleSheet::parse(css);

    let mut row = Row::new().with_child(Label::new("hi"));
    let buf = render_with_sheet(&mut row, 8, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}

#[test]
fn nested_amp_class_selector_applies() {
    let css = r#"
    Label {
        &.notice { bold: true; }
    }
    "#;
    let sheet = StyleSheet::parse(css);

    let mut label = Container::new().with_child(Label::new("hi").class("notice"));
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn nested_parent_and_child_rules_both_apply() {
    let css = r#"
    Label {
        color: red;
        &.notice { bold: true; }
    }
    "#;
    let sheet = StyleSheet::parse(css);

    let mut label = Container::new().with_child(Label::new("hi").class("notice"));
    let buf = render_with_sheet(&mut label, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(
        style.color,
        Some(Color::parse("red").expect("parse red").to_simple_opaque())
    );
    assert_eq!(style.bold, Some(true));
}
