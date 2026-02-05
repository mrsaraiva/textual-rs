use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

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

#[test]
fn scroll_view_horizontal_uses_child_intrinsic_width() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let mut scroll = ScrollView::new(Label::new("alpha-bravo")).height(1);
    let before = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let before_lines = before.as_plain_lines();
    scroll.scroll_by_x(6);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let lines = buf.as_plain_lines();
    assert_ne!(lines, before_lines);
    assert!(!lines[0].starts_with("alpha"), "got {:?}", lines[0]);
}

#[test]
fn horizontal_scroll_container_scrolls_long_lines() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let mut scroll = HorizontalScroll::new()
        .with_child(Static::new("alpha-bravo"))
        .height(1);
    let _ = FrameBuffer::from_renderable(&console, &options, &scroll, None);

    let mut ctx = EventCtx::default();
    scroll.on_mouse_scroll(0, 3, &mut ctx);
    assert!(ctx.handled());

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let lines = buf.as_plain_lines();
    assert_ne!(lines[0], "alpha-br");
}
