use rich_rs::Console;
use textual::prelude::*;
use textual::event::EventCtx;
use textual::render::FrameBuffer;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn key_panel_renders_bindings() {
    let console = Console::new();
    let options = options_for(&console, 32, 4);
    let panel = KeyPanel::new().with_bindings(vec![
        FooterBinding::new("^q", "Quit application"),
        FooterBinding::new("⇥", "Focus next"),
    ]);

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("^q")));
    assert!(lines.iter().any(|line| line.contains("Quit application")));
}

#[test]
fn key_panel_scrolls_with_actions() {
    let console = Console::new();
    let options = options_for(&console, 36, 3);
    let mut panel = KeyPanel::new().with_bindings(vec![
        FooterBinding::new("a", "one"),
        FooterBinding::new("b", "two"),
        FooterBinding::new("c", "three"),
        FooterBinding::new("d", "four"),
    ]);

    let before = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let before_lines = before.as_plain_lines();
    assert!(before_lines.iter().all(|line| !line.contains("four")));

    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); panel.on_event(&Event::Action(Action::ScrollDown), &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); panel.on_event(&Event::Action(Action::ScrollDown), &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); panel.on_event(&Event::Action(Action::ScrollDown), &mut __w) };

    let after = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let after_lines = after.as_plain_lines();
    assert!(after_lines.iter().any(|line| line.contains("four")));
}

#[test]
fn key_panel_updates_on_bindings_changed_event() {
    let console = Console::new();
    let options = options_for(&console, 36, 4);
    let mut panel = KeyPanel::new().with_bindings(vec![FooterBinding::new("a", "alpha")]);

    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); panel.on_event(
        &Event::BindingsChanged(vec![BindingHint::new("x, y", "Updated action")]),
        &mut __w) };

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("x y")));
    assert!(lines.iter().any(|line| line.contains("Updated action")));
}

#[test]
fn bindings_table_layout_height_matches_row_count_without_synthetic_header() {
    let table = BindingsTable::new().with_bindings(vec![
        FooterBinding::new("a", "one"),
        FooterBinding::new("b", "two"),
    ]);
    assert_eq!(table.layout_height(), Some(2));
}

#[test]
fn key_panel_does_not_consume_scroll_actions_without_overflow() {
    let mut panel = KeyPanel::new().with_bindings(vec![FooterBinding::new("a", "alpha")]);
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); panel.on_event(&Event::Action(Action::ScrollDown), &mut __w) };
    assert!(!ctx.handled());
}

#[test]
fn key_panel_supports_scrollbar_drag() {
    let console = Console::new();
    let options = options_for(&console, 32, 6);
    let bindings = (1..=16)
        .map(|index| FooterBinding::new(format!("k{index:02}"), format!("item {index:02}")))
        .collect::<Vec<_>>();
    let mut panel = KeyPanel::new().with_bindings(bindings);

    let before = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let before_lines = before.as_plain_lines();
    assert!(before_lines.iter().all(|line| !line.contains("item 16")));

    let _ = panel.compose();
    let _ = panel.render(&console, &options);

    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); panel.on_message(
        &MessageEvent::new(
            NodeId::default(),
            ScrollbarScrollTo {
                axis: ScrollbarAxis::Vertical,
                offset: 10.0,
                animate: false,
                scroll_duration: None,
            },
        ),
        &mut __w) };
    assert!(ctx.handled());
    assert!(ctx.repaint_requested());

    let after = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let after_lines = after.as_plain_lines();
    assert!(after_lines.iter().any(|line| line.contains("item 16")));
}
