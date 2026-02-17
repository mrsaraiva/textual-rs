use rich_rs::Console;
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame_with_stylesheet};

#[test]
fn stylesheet_applies_type_and_id_styles() {
    let console = Console::new();
    let mut label = Node::new(Label::new("hi")).id("hero");

    let mut sheet = StyleSheet::new();
    sheet.add_type("Node", Style::new().bold(true));
    sheet.add_id("hero", Style::new().underline(true));

    let mut tree = build_widget_tree_from_root(&mut label).expect("tree should exist");
    let buf = render_tree_to_frame_with_stylesheet(&mut tree, &mut label, &console, 6, 1, sheet);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style to be set");
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
}
