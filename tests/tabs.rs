use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::parse_color_like;

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

#[test]
fn tabs_mouse_hit_testing_handles_wide_grapheme_titles() {
    let mut tabs = Tabs::new()
        .with_tab("👩‍🚀", Label::new("first"))
        .with_tab("Deux", Label::new("second"));
    tabs.on_layout(40, 5);
    let id = tabs.id();
    let first_label_cells = rich_rs::cell_len(" 👩‍🚀 ");
    let mut ctx = EventCtx::default();
    tabs.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: first_label_cells as u16 + 1,
            screen_y: 0,
            x: first_label_cells as u16 + 1,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());
    assert_eq!(tabs.active(), 1);
}

#[test]
fn tabs_default_css_focus_styles_active_tab_and_underline() {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (20, 2);
    options.max_width = 20;
    options.max_height = 2;

    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));
    tabs.set_focus(true);
    tabs.on_layout(20, 2);

    let buf = FrameBuffer::from_renderable(&console, &options, &tabs, None);

    let active_tab_style = buf.get(1, 0).style.expect("active tab style");
    assert_eq!(
        active_tab_style.bgcolor,
        Some(
            parse_color_like("$block-cursor-background")
                .expect("block cursor background")
                .to_simple_opaque()
        )
    );
    assert_eq!(
        active_tab_style.color,
        Some(
            parse_color_like("$block-cursor-foreground")
                .expect("block cursor foreground")
                .to_simple_opaque()
        )
    );
    assert_eq!(active_tab_style.reverse, Some(true));

    let active_underline_style = buf.get(1, 1).style.expect("active underline style");
    let inactive_underline_style = buf.get(12, 1).style.expect("inactive underline style");
    let focused_underline_bg = parse_color_like("$surface-lighten-1")
        .expect("focused underline background")
        .to_simple_opaque();
    assert_eq!(active_underline_style.bgcolor, Some(focused_underline_bg));
    assert_eq!(inactive_underline_style.bgcolor, Some(focused_underline_bg));
}

#[test]
fn tabs_keyboard_navigation_skips_disabled_and_hidden_tabs() {
    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"))
        .with_tab("Three", Label::new("third"))
        .with_tab("Four", Label::new("fourth"));
    assert!(tabs.disable_tab(1));
    assert!(tabs.hide_tab(2));
    tabs.set_focus(true);

    let right = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right.clone()), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(tabs.active(), 3);

    let mut wrap_ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right), &mut wrap_ctx);
    assert!(wrap_ctx.handled());
    assert_eq!(tabs.active(), 0);
}

#[test]
fn tabs_mouse_click_disabled_tab_does_not_activate() {
    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));
    assert!(tabs.disable_tab(1));
    tabs.on_layout(40, 5);
    let id = tabs.id();
    let mut ctx = EventCtx::default();
    tabs.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 6,
            screen_y: 0,
            x: 6,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(!ctx.handled());
    assert_eq!(tabs.active(), 0);
}

#[test]
fn tabs_hiding_active_tab_promotes_next_available() {
    let mut tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"))
        .with_tab("Three", Label::new("third"));
    tabs.set_active(1);
    assert_eq!(tabs.active(), 1);

    assert!(tabs.hide_tab(1));
    assert_eq!(tabs.active(), 2);
    assert!(tabs.hide_tab(2));
    assert_eq!(tabs.active(), 0);
}
