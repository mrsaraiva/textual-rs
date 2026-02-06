use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::{MouseDownEvent, MouseUpEvent};
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn checkbox_toggles_from_keyboard_and_emits_message() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 1);
    options.max_width = 16;
    options.max_height = 1;

    let mut checkbox = Checkbox::new("remember me");
    checkbox.set_focus(true);

    let key =
        KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()));
    let mut ctx = EventCtx::default();
    checkbox.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());
    assert!(checkbox.checked());

    let buf = FrameBuffer::from_renderable(&console, &options, &checkbox, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn checkbox_click_activates_only_on_mouse_up_over_target() {
    let mut checkbox = Checkbox::new("remember me");
    let id = checkbox.id();
    checkbox.set_focus(true);
    checkbox.set_hovered(true);

    let mut ctx = EventCtx::default();
    checkbox.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());
    assert!(!checkbox.checked());

    let mut ctx = EventCtx::default();
    checkbox.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(id),
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());
    assert!(checkbox.checked());
}

#[test]
fn checkbox_disabled_ignores_input() {
    let mut checkbox = Checkbox::new("remember me").disabled(true);
    checkbox.set_focus(true);
    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    checkbox.on_event(&Event::Key(key), &mut ctx);
    assert!(!checkbox.checked());
    assert!(!ctx.handled());
}
