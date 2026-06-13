use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::MouseDownEvent;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;
use textual::widgets::NodeState;

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
    welcome.on_layout(48, 10);

    let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    // Focus state is managed by the runtime; inject it for this test.
    let focused_state = NodeState {
        focused: true,
        ..Default::default()
    };
    let _guard = set_dispatch_recipient(NodeId::default(), focused_state);
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
fn welcome_unmount_does_not_panic() {
    // Hover/focus state is managed by the runtime node record (NodeState).
    // Verify that unmount completes without panicking.
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 6);
    welcome.on_unmount();
    // After unmount, node_state() defaults to all-false (no dispatch guard active).
    assert!(!welcome.node_state().hovered);
}

#[test]
fn welcome_unmount_does_not_panic_after_state_change() {
    // Focus/hover state lives on the node record, not the widget struct.
    // on_node_state_changed propagates state changes to internal sub-widgets.
    // on_unmount must still complete without panic.
    let mut welcome = Welcome::new();
    welcome.on_layout(32, 6);
    welcome.on_node_state_changed(
        NodeState::default(),
        NodeState {
            focused: true,
            hovered: true,
            ..Default::default()
        },
    );

    welcome.on_unmount();

    // After unmount (and with no active dispatch guard), node_state() returns defaults.
    assert!(!welcome.node_state().focused);
    assert!(!welcome.node_state().hovered);
}
