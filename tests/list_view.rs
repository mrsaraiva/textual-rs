use rich_rs::Console;
use slotmap::SlotMap;
use textual::event::MouseDownEvent;
use textual::event::EventCtx;
use textual::prelude::*;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

fn focused_state() -> NodeState {
    NodeState {
        focused: true,
        ..Default::default()
    }
}

/// Render a root widget through the arena tree (the canonical rendering path).
fn render_root(root: &mut dyn Widget, width: usize, height: usize) -> Vec<String> {
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    let buf = render_tree_to_frame(&mut tree, root, &console, width, height);
    buf.as_plain_lines()
}

// ── Arena composition / rendering ────────────────────────────────────────

#[test]
fn list_view_renders_items_without_marker() {
    // ListView composes ListItem(Label) children; no `› ` cursor marker.
    let mut root = Container::new().with_child(ListView::from_list_items(vec![
        ListItem::new(Label::new("One")),
        ListItem::new(Label::new("Two")),
        ListItem::new(Label::new("Three")),
    ]));
    let lines = render_root(&mut root, 12, 3);
    let joined = lines.join("\n");
    assert!(!joined.contains('\u{203a}'), "no `›` marker: {joined:?}");
    assert!(joined.contains("One"), "items render: {joined:?}");
    assert!(joined.contains("Two"));
    assert!(joined.contains("Three"));
}

#[test]
fn list_view_items_span_three_rows_with_label_padding() {
    // Python docs example: `Label { padding: 1 2 }` makes each ListItem 3 rows
    // tall (top pad / content / bottom pad). Verify the composed layout.
    const CSS: &str = "Label { padding: 1 2; }";
    let sheet = {
        let mut s = textual::css::default_widget_stylesheet();
        s.extend(&StyleSheet::parse(CSS));
        s
    };
    let _guard = textual::css::set_style_context(sheet.clone());
    let console = Console::new();
    let mut root = Container::new().with_child(ListView::from_list_items(vec![
        ListItem::new(Label::new("One")),
        ListItem::new(Label::new("Two")),
    ]));
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree");
    let buf = render_tree_to_frame_with_stylesheet(&mut tree, &mut root, &console, 12, 6, sheet);
    let lines = buf.as_plain_lines();
    // 2 items * 3 rows = 6 lines. "One" sits on the middle row of its item.
    assert_eq!(lines.len(), 6, "{lines:?}");
    assert!(lines[1].contains("One"), "row 1 holds One: {lines:?}");
    assert!(lines[4].contains("Two"), "row 4 holds Two: {lines:?}");
    // The padding rows around the text are blank (no marker, no content).
    assert!(!lines[0].contains("One"));
    assert!(!lines[2].contains("One"));
}

#[test]
fn list_view_highlight_is_background_only() {
    // The highlighted item carries the `-highlight` class (bg-only); there is
    // no text marker. Drive the highlight via the same per-child class the
    // runtime sync applies, then confirm the item node has the class.
    let mut list = ListView::from_list_items(vec![
        ListItem::new(Label::new("One")),
        ListItem::new(Label::new("Two")),
    ]);
    list.set_selected(1);
    let classes = list.child_classes_for_tree(1);
    assert!(classes.contains(&("-highlight", true)));
    let classes0 = list.child_classes_for_tree(0);
    assert!(classes0.contains(&("-highlight", false)));
}

// ── Selection / navigation behaviour (state model) ───────────────────────

#[test]
fn list_view_mouse_click_selects_row_headless() {
    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
        "four".to_string(),
    ]);
    list.on_layout(20, 3);
    let id = NodeId::default();
    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        list.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut __w);
    }
    assert!(ctx.handled());
    assert_eq!(list.selected(), 1);
}

#[test]
fn list_view_scroll_actions_keep_selection_in_state() {
    let mut list = ListView::new((0..20).map(|idx| format!("item-{idx}")).collect());
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    list.on_layout(20, 4);
    let mut ctx = EventCtx::default();
    for _ in 0..7 {
        {
            let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::Action(Action::ScrollDown), &mut __w);
        }
    }
    assert_eq!(list.selected(), 7);
    assert_eq!(list.selected_item(), Some("item-7"));
}

#[test]
fn list_view_mouse_scroll_clamps_to_bounds() {
    let mut list = ListView::new((0..10).map(|idx| format!("item-{idx}")).collect());
    list.on_layout(20, 3);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        list.on_mouse_scroll(0, 100, &mut __w);
    }
    assert!(ctx.handled());
    assert_eq!(list.offset(), 7);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        list.on_mouse_scroll(0, -100, &mut __w);
    }
    assert!(ctx.handled());
    assert_eq!(list.offset(), 0);
}

#[test]
fn list_view_navigation_skips_disabled_items() {
    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    list.set_item_disabled(1, true);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    list.on_layout(20, 3);

    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        list.on_event(&Event::Action(Action::ScrollDown), &mut __w);
    }
    assert_eq!(list.selected(), 2);
}

#[test]
fn list_view_mouse_click_ignores_disabled_items() {
    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    list.set_item_disabled(1, true);
    list.on_layout(20, 3);

    let id = NodeId::default();
    let mut ctx = EventCtx::default();
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        list.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut __w);
    }

    assert!(!ctx.handled());
    assert_eq!(list.selected(), 0);
}

#[test]
fn list_view_append_after_mount_recomposes_all_items() {
    // After the first mount the owned items are drained; appending and
    // recomposing must rebuild every item from the retained text, not just the
    // appended one.
    let mut list = ListView::new(vec!["A".to_string(), "B".to_string()]);
    let _first = Widget::compose(&mut list);
    list.append("C".to_string());
    let children = Widget::compose(&mut list);
    assert_eq!(children.len(), 3, "all three items recomposed");
}
