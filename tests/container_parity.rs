use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

// TODO(parity): container DEFAULT_CSS semantics that depend on TCSS properties like
// `layout`, `overflow`, `align-horizontal`, and `align-vertical` still need parser/runtime support.
// These tests lock in safe API/lifecycle parity slices until that broader CSS parity lands.

#[test]
fn vertical_alias_stacks_children() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let vertical = Vertical::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("beta"));
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&vertical), None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("alpha"), "first line: {:?}", lines[0]);
    assert!(lines[1].starts_with("beta"), "second line: {:?}", lines[1]);
}

#[test]
fn center_alias_aligns_children_horizontally() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (9, 1);
    options.max_width = 9;
    options.max_height = 1;

    let center = Center::new().with_child(Label::new("cat"));
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&center), None);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "   cat   ");
}

#[test]
fn right_alias_aligns_children_horizontally() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let right = Right::new().with_child(Label::new("dog"));
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&right), None);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "     dog");
}

#[test]
fn middle_alias_centers_children_vertically() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (5, 5);
    options.max_width = 5;
    options.max_height = 5;

    let middle = Middle::new().with_child(Label::new("x"));
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&middle), None);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "     ");
    assert_eq!(lines[1], "     ");
    assert_eq!(lines[2], "x    ");
    assert_eq!(lines[3], "     ");
    assert_eq!(lines[4], "     ");
}

#[test]
fn scrollview_is_focusable_and_supports_home_end_actions() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let list = ListView::new(vec![
        "item 1".to_string(),
        "item 2".to_string(),
        "item 3".to_string(),
        "item 4".to_string(),
    ]);
    let mut scroll = ScrollView::new(list).height(2);
    assert!(scroll.focusable());
    let _ = FrameBuffer::from_renderable(&console, &options, &scroll, None);

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollEnd), &mut ctx);
    assert!(ctx.handled());
    assert!(scroll.offset_y() > 0);

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollHome), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(scroll.offset_y(), 0);
}

#[test]
fn vertical_scroll_supports_home_end_actions() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let mut scroll = VerticalScroll::new()
        .with_child(Static::new("row 1"))
        .with_child(Static::new("row 2"))
        .with_child(Static::new("row 3"))
        .with_child(Static::new("row 4"))
        .height(2);
    assert!(scroll.focusable());
    let _ = FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&scroll), None);

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollEnd), &mut ctx);
    assert!(ctx.handled());
    let end =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&scroll), None);
    let end_lines = end.as_plain_lines();
    assert!(
        end_lines[0].starts_with("row 3"),
        "line: {:?}",
        end_lines[0]
    );

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollHome), &mut ctx);
    assert!(ctx.handled());
    let home =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&scroll), None);
    let home_lines = home.as_plain_lines();
    assert!(
        home_lines[0].starts_with("row 1"),
        "line: {:?}",
        home_lines[0]
    );
}

#[test]
fn horizontal_scroll_is_focusable_and_supports_home_end_actions() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let mut scroll = HorizontalScroll::new()
        .with_child(Static::new("abcdefghi"))
        .height(1);
    assert!(scroll.focusable());
    let _ = FrameBuffer::from_renderable(&console, &options, &scroll, None);

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollEnd), &mut ctx);
    assert!(ctx.handled());
    let end = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let end_lines = end.as_plain_lines();
    assert!(
        !end_lines[0].starts_with("abcdef"),
        "line: {:?}",
        end_lines[0]
    );

    let mut ctx = EventCtx::default();
    scroll.on_event(&Event::Action(Action::ScrollHome), &mut ctx);
    assert!(ctx.handled());
    let home = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let home_lines = home.as_plain_lines();
    assert!(
        home_lines[0].starts_with("abcdef"),
        "line: {:?}",
        home_lines[0]
    );
}
