use rich_rs::Console;
use textual::prelude::*;
use textual::event::EventCtx;
use textual::render::FrameBuffer;
use textual::widget_tree::WidgetTree;

fn render_once(root: &mut dyn Widget, width: usize, height: usize) -> FrameBuffer {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    render_tree_to_frame(&mut tree, root, &console, width, height)
}

fn render_with_tree(
    tree: &mut WidgetTree,
    root: &mut dyn Widget,
    console: &Console,
    width: usize,
    height: usize,
) -> FrameBuffer {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    render_tree_to_frame(tree, root, console, width, height)
}

#[test]
fn vertical_alias_stacks_children() {
    let mut vertical = Vertical::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("beta"));
    let buf = render_once(&mut vertical, 8, 2);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("alpha"), "first line: {:?}", lines[0]);
    assert!(lines[1].starts_with("beta"), "second line: {:?}", lines[1]);
}

#[test]
fn vertical_group_alias_stacks_children() {
    let mut vertical = VerticalGroup::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("beta"));
    let buf = render_once(&mut vertical, 8, 2);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("alpha"), "first line: {:?}", lines[0]);
    assert!(lines[1].starts_with("beta"), "second line: {:?}", lines[1]);
}

#[test]
fn center_alias_aligns_children_horizontally() {
    let mut center = Center::new().with_child(Label::new("cat").with_shrink(true));
    let buf = render_once(&mut center, 9, 1);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "   cat   ");
}

#[test]
fn horizontal_group_alias_places_children_in_row() {
    let mut row = HorizontalGroup::new()
        .with_child(Label::new("a"))
        .with_child(Label::new("b"));
    let buf = render_once(&mut row, 8, 1);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("a"));
    assert!(line.contains("b"));
}

#[test]
fn right_alias_aligns_children_horizontally() {
    let mut right = Right::new().with_child(Label::new("dog").with_shrink(true));
    let buf = render_once(&mut right, 8, 1);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "     dog");
}

#[test]
fn middle_alias_centers_children_vertically() {
    let mut middle = Middle::new().with_child(Label::new("x"));
    let buf = render_once(&mut middle, 5, 5);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[0], "     ");
    assert_eq!(lines[1], "     ");
    assert_eq!(lines[2], "x    ");
    assert_eq!(lines[3], "     ");
    assert_eq!(lines[4], "     ");
}

#[test]
fn center_middle_centers_both_axes() {
    let mut center_middle = CenterMiddle::new().with_child(Label::new("ok").with_shrink(true));
    let buf = render_once(&mut center_middle, 7, 5);
    let lines = buf.as_plain_lines();
    assert_eq!(lines[2], "  ok   ");
}

#[test]
fn scrollview_is_focusable_and_supports_home_end_actions() {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut scroll = ScrollView::new(ListView::new(vec![
        "item 1".to_string(),
        "item 2".to_string(),
        "item 3".to_string(),
        "item 4".to_string(),
    ]))
    .height(2);
    assert!(scroll.focusable());
    let mut tree = build_widget_tree_from_root(&mut scroll).expect("tree should build");
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 8, 2);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollEnd), &mut __w);
    }
    assert!(ctx.handled());
    assert!(scroll.offset_y() > 0);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollHome), &mut __w);
    }
    assert!(ctx.handled());
    assert_eq!(scroll.offset_y(), 0);
}

#[test]
fn vertical_scroll_supports_home_end_actions() {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut scroll = VerticalScroll::new()
        .with_child(Static::new("row 1"))
        .with_child(Static::new("row 2"))
        .with_child(Static::new("row 3"))
        .with_child(Static::new("row 4"))
        .height(2);
    assert!(scroll.focusable());
    let mut tree = build_widget_tree_from_root(&mut scroll).expect("tree should build");
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 8, 2);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollEnd), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 8, 2);
    assert!(scroll.scroll_offset().1 > 0);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollHome), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 8, 2);
    assert_eq!(scroll.scroll_offset().1, 0);
}

#[test]
fn scrollable_container_supports_home_end_actions() {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut scrollable = ScrollableContainer::new()
        .with_child(Static::new("row 1"))
        .with_child(Static::new("row 2"))
        .with_child(Static::new("row 3"))
        .with_child(Static::new("row 4"))
        .height(2);
    assert!(scrollable.focusable());
    let mut tree = build_widget_tree_from_root(&mut scrollable).expect("tree should build");
    let _ = render_with_tree(&mut tree, &mut scrollable, &console, 8, 2);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scrollable.on_event(&Event::Action(Action::ScrollEnd), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scrollable, &console, 8, 2);
    assert!(scrollable.scroll_offset().1 > 0);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scrollable.on_event(&Event::Action(Action::ScrollHome), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scrollable, &console, 8, 2);
    assert_eq!(scrollable.scroll_offset().1, 0);
}

#[test]
fn horizontal_scroll_is_focusable_and_supports_home_end_actions() {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut scroll = HorizontalScroll::new()
        .with_child(Label::new("abcdefghi").with_shrink(true))
        .height(1);
    assert!(scroll.focusable());
    let mut tree = build_widget_tree_from_root(&mut scroll).expect("tree should build");
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 6, 1);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollEnd), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 6, 1);
    assert!(scroll.scroll_offset().0 > 0);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        scroll.on_event(&Event::Action(Action::ScrollHome), &mut __w);
    }
    assert!(ctx.handled());
    let _ = render_with_tree(&mut tree, &mut scroll, &console, 6, 1);
    assert_eq!(scroll.scroll_offset().0, 0);
}

#[test]
fn item_grid_renders_cells() {
    let mut grid = ItemGrid::new()
        .with_child(Label::new("a"))
        .with_child(Label::new("b"));
    let buf = render_once(&mut grid, 10, 2);
    let rendered = buf.as_plain_lines().join("\n");
    assert!(rendered.contains('a'));
    assert!(rendered.contains('b'));
}
