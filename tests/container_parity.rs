use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

// DEFERRED(parity): container DEFAULT_CSS semantics depend on missing TCSS layout properties.
// See ROADMAP.md "TCSS Property Parity Audit" — Tier 1 (position, box-sizing) and Tier 2
// (individual padding/margin sides) are the primary blockers for full container CSS parity.

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
fn vertical_group_alias_stacks_children() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let vertical = VerticalGroup::new()
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
fn horizontal_group_alias_places_children_in_row() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let row = HorizontalGroup::new()
        .with_child(Label::new("a"))
        .with_child(Label::new("b"));
    let buf = FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&row), None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("a"));
    assert!(line.contains("b"));
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
fn center_middle_centers_both_axes() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (7, 5);
    options.max_width = 7;
    options.max_height = 5;

    let center_middle = CenterMiddle::new().with_child(Label::new("ok"));
    let buf = FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&center_middle),
        None,
    );
    let lines = buf.as_plain_lines();
    assert_eq!(lines[2], "  ok   ");
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
fn scrollable_container_supports_home_end_actions() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let mut scrollable = ScrollableContainer::new()
        .with_child(Static::new("row 1"))
        .with_child(Static::new("row 2"))
        .with_child(Static::new("row 3"))
        .with_child(Static::new("row 4"))
        .height(2);
    assert!(scrollable.focusable());
    let _ = FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&scrollable),
        None,
    );

    let mut ctx = EventCtx::default();
    scrollable.on_event(&Event::Action(Action::ScrollEnd), &mut ctx);
    assert!(ctx.handled());
    let end = FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&scrollable),
        None,
    );
    let end_lines = end.as_plain_lines();
    assert!(end_lines[0].starts_with("row 3"));

    let mut ctx = EventCtx::default();
    scrollable.on_event(&Event::Action(Action::ScrollHome), &mut ctx);
    assert!(ctx.handled());
    let home = FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&scrollable),
        None,
    );
    let home_lines = home.as_plain_lines();
    assert!(home_lines[0].starts_with("row 1"));
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

#[test]
fn item_grid_renders_cells() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 2);
    options.max_width = 10;
    options.max_height = 2;

    let grid = ItemGrid::new(1, 2)
        .with_cell(0, 0, Label::new("a"))
        .with_cell(0, 1, Label::new("b"));
    let buf = FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&grid), None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].contains("a"));
    assert!(lines[0].contains("b"));
}
