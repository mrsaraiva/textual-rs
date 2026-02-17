use rich_rs::Console;
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

fn render_lines_tree(root: &mut dyn Widget, width: usize, height: usize) -> Vec<String> {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should exist");
    let buffer = render_tree_to_frame(&mut tree, root, &console, width, height);
    buffer.as_plain_lines()
}

#[test]
fn preview_root_with_top_and_bottom_renders_sections() {
    let mut root = preview_root_with_top_bottom(
        Some("Preview"),
        Some(2),
        Label::new("Top"),
        Label::new("Body"),
        Some(2),
        Label::new("Bottom"),
    );

    let lines = render_lines_tree(&mut root, 40, 8);
    assert!(lines[0].contains("Preview"));
    assert!(lines.iter().any(|line| line.contains("Top")));
    assert!(lines.iter().any(|line| line.contains("Body")));
    assert!(lines.iter().any(|line| line.contains("Bottom")));
}

#[test]
fn preview_root_without_title_skips_header() {
    let mut root = preview_root(None, Label::new("Only body"));

    let lines = render_lines_tree(&mut root, 24, 3);
    assert!(lines[0].contains("Only body"));
    assert!(lines.iter().all(|line| !line.contains("Textual")));
}
