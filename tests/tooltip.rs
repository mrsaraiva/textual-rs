use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::MouseScrollEvent;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn tooltip_renders_overlay_text_when_visible() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let tooltip = Tooltip::new(Label::new("base content"), "Tooltip text").visible(true);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("Tooltip text")));
    assert!(lines.iter().any(|line| line.contains("base content")));
}

#[test]
fn tooltip_hides_on_escape_key() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip").visible(true);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tooltip.on_event(&Event::Key(key), &mut EventCtx::default());

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().all(|line| !line.contains("tip")));
    assert!(lines.iter().any(|line| line.contains("base")));
}

#[test]
fn tooltip_visibility_can_be_driven_via_overlay_messages() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip").visible(true);

    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlaySetVisible {
                overlay: tooltip.id(),
                visible: false,
            },
        },
        &mut EventCtx::default(),
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().all(|line| !line.contains("tip")));
}

#[test]
fn tooltip_positions_above_anchor_when_bottom_space_is_insufficient() {
    let console = Console::new();
    let options = options_for(&console, 28, 6);
    let tooltip = Tooltip::new(Label::new("base"), "anchored")
        .visible(true)
        .with_anchor(14, 5);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    let line_idx = lines
        .iter()
        .position(|line| line.contains("anchored"))
        .expect("tooltip line");

    assert_eq!(line_idx, 2);
}

#[test]
fn tooltip_clamps_horizontally_when_anchor_is_left_of_viewport() {
    let console = Console::new();
    let options = options_for(&console, 20, 6);
    let tooltip = Tooltip::new(Label::new("base"), "left-edge")
        .visible(true)
        .with_anchor(0, 1);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    let line = lines
        .iter()
        .find(|line| line.contains("left-edge"))
        .expect("tooltip line");
    let x = line.find("left-edge").expect("x position");

    assert_eq!(x, 2);
}

#[test]
fn tooltip_updates_anchor_from_runtime_mouse_events() {
    let console = Console::new();
    let options = options_for(&console, 30, 8);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip")
        .visible(true)
        .with_anchor(2, 0);

    let before = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let before_lines = before.as_plain_lines();
    let before_line = before_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line before");
    let before_x = before_line.find("tip").expect("tip x before");

    let target = tooltip.anchor_target_id();
    tooltip.on_event(
        &Event::MouseScroll(MouseScrollEvent {
            target: Some(target),
            screen_x: 22,
            screen_y: 1,
            x: 22,
            y: 1,
            delta_x: 0,
            delta_y: 1,
            modifiers: KeyModifiers::empty(),
        }),
        &mut EventCtx::default(),
    );

    let after = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let after_lines = after.as_plain_lines();
    let after_line = after_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line after");
    let after_x = after_line.find("tip").expect("tip x after");

    assert!(after_x > before_x);
}
