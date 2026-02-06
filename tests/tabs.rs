use rich_rs::Console;
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn tabs_render_header_and_active_content() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (20, 3);
    options.max_width = 20;
    options.max_height = 3;

    let tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));

    let buf = FrameBuffer::from_renderable(&console, &options, &tabs, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn tabs_keyboard_changes_active_tab() {
    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));
    tabs.set_focus(true);
    let key = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(tabs.active(), 1);
}

#[test]
fn tabs_mouse_click_on_header_changes_active_tab() {
    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));
    tabs.on_layout(40, 5);
    let id = tabs.id();
    let mut ctx = EventCtx::default();
    tabs.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());
    assert_eq!(tabs.active(), 1);
}
