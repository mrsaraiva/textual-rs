use rich_rs::Console;
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

#[test]
fn preview_root_top_bottom_snapshot() {
    let console = Console::new();

    let mut root = preview_root_with_top_bottom(
        Some("Preview"),
        Some(2),
        Label::new("Top panel"),
        Label::new("Main body"),
        Some(2),
        Label::new("Bottom panel"),
    );

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should exist");
    let buf = render_tree_to_frame(&mut tree, &mut root, &console, 48, 10);
    insta::assert_snapshot!(buf.debug_dump());
}
