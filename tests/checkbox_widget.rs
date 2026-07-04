use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::{MouseDownEvent, MouseUpEvent};
use textual::event::EventCtx;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;
use textual::widgets::NodeState;

fn focused_state() -> NodeState {
    NodeState {
        focused: true,
        ..Default::default()
    }
}

#[test]
fn checkbox_toggles_from_keyboard_and_emits_message() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 1);
    options.max_width = 16;
    options.max_height = 1;

    let mut checkbox = Checkbox::new("remember me");
    let id = NodeId::default();
    let _guard = set_dispatch_recipient(id, focused_state());

    let key =
        KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()));
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); checkbox.on_event(&Event::Key(key), &mut __w) };
    assert!(ctx.handled());
    assert!(checkbox.checked());

    let buf = FrameBuffer::from_renderable(&console, &options, &checkbox, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn checkbox_click_activates_only_on_mouse_up_over_target() {
    let mut checkbox = Checkbox::new("remember me");
    let id = NodeId::default();
    let _guard = set_dispatch_recipient(
        id,
        NodeState {
            hovered: true,
            ..Default::default()
        },
    );

    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); checkbox.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut __w) };
    assert!(ctx.handled());
    assert!(!checkbox.checked());

    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); checkbox.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(id),
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut __w) };
    assert!(ctx.handled());
    assert!(checkbox.checked());
}

#[test]
fn checkbox_disabled_ignores_input() {
    let mut checkbox = Checkbox::new("remember me").disabled(true);
    let id = NodeId::default();
    let _guard = set_dispatch_recipient(id, focused_state());
    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); checkbox.on_event(&Event::Key(key), &mut __w) };
    assert!(!checkbox.checked());
    assert!(!ctx.handled());
}
