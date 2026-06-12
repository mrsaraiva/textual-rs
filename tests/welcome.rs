use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::MouseDownEvent;
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
fn welcome_renders_title_and_close_button() {
    let console = Console::new();
    let options = options_for(&console, 72, 12);
    let mut welcome = Welcome::new();
    welcome.on_layout(72, 12);

    let buf = FrameBuffer::from_renderable(&console, &options, &welcome, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("Welcome!")));
    assert!(lines.iter().any(|line| line.contains("OK")));
}

#[test]
fn welcome_re_emits_button_press_from_widget_sender() {
    let mut welcome = Welcome::new();
    welcome.on_layout(48, 10);

    let mut ctx = EventCtx::default();
    welcome.on_message(
        &MessageEvent::new(
            welcome.close_button_id(),
            ButtonPressed {
                description: "Button(classes='button', variant='success')".to_string(),
                button_id: None,
            },
        ),
        &mut ctx,
    );
    assert!(ctx.handled());
}

#[test]
fn welcome_key_press_is_forwarded_to_close_button() {
    let mut welcome = Welcome::new();
    welcome.set_focus(true);
    welcome.on_layout(48, 10);

    let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    welcome.on_event(&Event::Key(enter), &mut ctx);

    assert!(ctx.handled());
}

#[test]
fn welcome_resize_updates_close_row_hit_testing() {
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 6);
    welcome.on_resize(32, 2);

    let mut ctx = EventCtx::default();
    welcome.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 1,
            screen_y: 1,
            x: 1,
            y: 1,
        }),
        &mut ctx,
    );

    assert!(ctx.handled());
}

#[test]
fn welcome_single_row_layout_routes_mouse_to_close_button() {
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 1);

    let mut ctx = EventCtx::default();
    welcome.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 0,
            x: 2,
            y: 0,
        }),
        &mut ctx,
    );

    assert!(ctx.handled());
}

#[test]
fn welcome_clears_hover_state_on_unmount() {
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 6);
    welcome.set_hovered(true);
    assert!(welcome.is_hovered());

    welcome.on_unmount();
    assert!(!welcome.is_hovered());
}

#[test]
fn welcome_unmount_resets_focus_and_hover_lifecycle_state() {
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 6);
    welcome.set_focus(true);
    welcome.set_hovered(true);

    welcome.on_unmount();

    assert!(!welcome.has_focus());
    assert!(!welcome.is_hovered());
}
