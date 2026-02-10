use rich_rs::Console;
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn list_view_renders_selection() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    list.set_focus(true);
    list.set_selected(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &list, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn list_view_mouse_click_selects_row() {
    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
        "four".to_string(),
    ]);
    list.on_layout(20, 3);
    let id = list.id();
    let mut ctx = EventCtx::default();
    list.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());
    assert_eq!(list.selected(), 1);
}

#[test]
fn list_view_scroll_actions_keep_selection_visible() {
    let mut list = ListView::new((0..20).map(|idx| format!("item-{idx}")).collect());
    list.set_focus(true);
    list.on_layout(20, 4);
    let mut ctx = EventCtx::default();
    for _ in 0..7 {
        list.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
    }
    assert_eq!(list.selected(), 7);

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 4);
    options.max_width = 16;
    options.max_height = 4;
    let buf = FrameBuffer::from_renderable(&console, &options, &list, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("item-7")));
}

#[test]
fn list_view_mouse_scroll_clamps_to_bounds() {
    let mut list = ListView::new((0..10).map(|idx| format!("item-{idx}")).collect());
    list.on_layout(20, 3);

    let mut ctx = EventCtx::default();
    list.on_mouse_scroll(0, 100, &mut ctx);
    assert!(ctx.handled());
    assert_eq!(list.offset(), 7);

    let mut ctx = EventCtx::default();
    list.on_mouse_scroll(0, -100, &mut ctx);
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
    list.set_focus(true);
    list.on_layout(20, 3);

    let mut ctx = EventCtx::default();
    list.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
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

    let id = list.id();
    let mut ctx = EventCtx::default();
    list.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );

    assert!(!ctx.handled());
    assert_eq!(list.selected(), 0);
}
