use rich_rs::Console;
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn tabbed_content_honors_initial_pane_id() {
    let mut tabs = TabbedContent::new()
        .initial("jessica")
        .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
        .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"));
    tabs.on_mount();
    assert_eq!(tabs.active_id(), Some("jessica"));
}

#[test]
fn tabbed_content_keyboard_changes_active_pane() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    tabs.set_focus(true);
    let key = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(tabs.active_id(), Some("two"));
}

#[test]
fn tabbed_content_mouse_click_header_changes_active_pane() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
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
    assert_eq!(tabs.active_id(), Some("two"));
}

#[test]
fn tabbed_content_component_id_css_selector_is_supported() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 2);
    options.max_width = 24;
    options.max_height = 2;
    let css = r#"
    TabbedContent #--content-tab-green { fg: green; }
    "#;
    let _guard = set_style_context(StyleSheet::parse(css));
    let tabs = TabbedContent::new()
        .with_pane(TabPane::new("Red", Label::new("red")).id("red"))
        .with_pane(TabPane::new("Green", Label::new("green")).id("green"));
    let _ = FrameBuffer::from_renderable(&console, &options, &tabs, None);
}
