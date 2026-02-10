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
