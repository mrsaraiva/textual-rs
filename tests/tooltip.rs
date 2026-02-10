use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
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
