use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

#[test]
fn scroll_view_renders_horizontal_offset() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let rows = ListView::new(vec!["alpha-bravo".to_string(), "charlie-delta".to_string()]);
    let mut scroll = ScrollView::new(rows).height(2);
    let before = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let before_lines = before.as_plain_lines();
    scroll.scroll_by_x(6);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let lines = buf.as_plain_lines();
    assert_ne!(lines, before_lines);
}

// A ScrollView whose child is wider than the viewport (here an 11-cell label in an
// 8-cell viewport) lays the child out at its intrinsic width instead of wrapping it, so the
// content overflows horizontally and can be scrolled. The child must report an intrinsic
// width — a plain `Label` intentionally fills/wraps (Textual parity), so use `with_shrink`.
#[test]
fn scroll_view_horizontal_uses_child_intrinsic_width() {
    let console = Console::new();
    let mut scroll = ScrollView::new(Label::new("alpha-bravo").with_shrink(true)).height(1);
    let mut tree = build_widget_tree_from_root(&mut scroll).expect("tree should build");
    let before_lines =
        render_tree_to_frame(&mut tree, &mut scroll, &console, 8, 1).as_plain_lines();
    // Initial view shows the start of the (unwrapped) content.
    assert_eq!(before_lines[0], "alpha-br", "got {:?}", before_lines[0]);

    scroll.scroll_by_x(6);
    let lines = render_tree_to_frame(&mut tree, &mut scroll, &console, 8, 1).as_plain_lines();

    // Scrolling advances the view (clamped to max offset 3 = content 11 - viewport 8) and
    // reveals the previously-hidden tail of the content.
    assert_ne!(lines, before_lines);
    assert_eq!(scroll.offset_x(), 3);
    assert_eq!(lines[0], "ha-bravo", "got {:?}", lines[0]);
}

#[test]
fn horizontal_scroll_container_scrolls_long_lines() {
    let console = Console::new();
    let mut scroll = HorizontalScroll::new()
        .with_child(Static::new("alpha-bravo"))
        .height(1);
    let mut tree = build_widget_tree_from_root(&mut scroll).expect("tree should build");
    let _ = render_tree_to_frame(&mut tree, &mut scroll, &console, 8, 1);
    scroll.scroll_by_x(3);
    let lines = render_tree_to_frame(&mut tree, &mut scroll, &console, 8, 1).as_plain_lines();
    assert_eq!(lines[0], "alpha-br");
}
