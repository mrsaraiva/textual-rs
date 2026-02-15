use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::reactive::ReactiveCtx;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};
use textual::style::parse_color_like;

fn two_tabs() -> Tabs {
    Tabs::new()
        .with_tab_id("one", "One")
        .with_tab_id("two", "Two")
}

fn render_tabs_frame(tabs: &mut Tabs, width: u16, height: u16) -> FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(tabs).expect("tree should exist");
    render_tree_to_frame(&mut tree, tabs, &console, width as usize, height as usize)
}

fn find_label_column(header: &str, label: &str) -> usize {
    header
        .find(label)
        .unwrap_or_else(|| panic!("label {label:?} should exist in header: {header:?}"))
        as usize
}

#[test]
fn tabs_render_header_and_active_content() {
    let mut tabs = two_tabs();
    let frame = render_tabs_frame(&mut tabs, 20, 3);
    let lines = frame.as_plain_lines();
    assert!(
        lines.first().is_some_and(|line| line.contains("One")),
        "first header row should include first tab label: {lines:?}"
    );
    assert!(
        lines.first().is_some_and(|line| line.contains("Two")),
        "first header row should include second tab label: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.chars().any(|ch| ch == '━')),
        "frame should contain underline glyphs: {lines:?}"
    );
}

#[test]
fn tabs_keyboard_changes_active_tab() {
    let mut tabs = two_tabs();
    tabs.set_focus(true);
    let key = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());
    assert!(tabs.is_active("two"));
}

#[test]
fn tabs_mouse_click_on_header_changes_active_tab() {
    let mut tabs = two_tabs();
    tabs.on_layout(40, 5);
    let id = NodeId::default();
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
    assert!(tabs.is_active("two"));
}

#[test]
fn tabs_mouse_hit_testing_handles_wide_grapheme_titles() {
    let mut tabs = Tabs::new()
        .with_tab_id("first", "👩‍🚀")
        .with_tab_id("deux", "Deux");
    tabs.on_layout(40, 5);
    let id = NodeId::default();
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
    assert!(tabs.is_active("deux"));
}

#[test]
fn tabs_switch_binding_hint_is_hidden_for_footer() {
    let tabs = two_tabs();
    assert_eq!(
        tabs.binding_hints(),
        vec![
            BindingHint::new("left/right", "Switch tab")
                .with_key_display("←/→")
                .hidden(true)
        ]
    );
}

#[test]
fn tabs_default_css_focus_styles_active_tab_and_underline() {
    let _guard = set_style_context(default_widget_stylesheet());
    let mut tabs = two_tabs();
    tabs.set_focus(true);
    let frame = render_tabs_frame(&mut tabs, 20, 2);

    let header = frame.as_plain_lines()[0].clone();
    let active_col = find_label_column(&header, "One");
    let inactive_col = find_label_column(&header, "Two");

    let active_tab_style = frame.get(active_col, 0).style.expect("active tab style");
    let inactive_tab_style = frame
        .get(inactive_col, 0)
        .style
        .expect("inactive tab style");
    assert_ne!(active_tab_style.color, inactive_tab_style.color);
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
                .flatten_over(
                    parse_color_like("$block-cursor-background").expect("block cursor background"),
                )
                .to_simple_opaque()
        )
    );
    assert_eq!(active_tab_style.bold, Some(true));

    let active_underline_style = frame
        .get(active_col, 1)
        .style
        .expect("active underline style");
    let inactive_underline_style = frame
        .get(inactive_col, 1)
        .style
        .expect("inactive underline style");
    assert!(
        active_underline_style.color.is_some(),
        "active underline should have explicit color style"
    );
    assert!(
        inactive_underline_style.color.is_some(),
        "inactive underline should have explicit color style"
    );
}

#[test]
fn tabs_default_css_styles_inactive_tab_differently_from_active_tab() {
    let _guard = set_style_context(default_widget_stylesheet());
    let mut tabs = two_tabs();
    let frame = render_tabs_frame(&mut tabs, 20, 2);

    let header = frame.as_plain_lines()[0].clone();
    let active_col = find_label_column(&header, "One");
    let inactive_col = find_label_column(&header, "Two");
    let active_style = frame.get(active_col, 0).style.expect("active style");
    let inactive_style = frame.get(inactive_col, 0).style.expect("inactive style");
    assert_ne!(active_style.color, inactive_style.color);
}

#[test]
fn tabs_keyboard_navigation_skips_disabled_and_hidden_tabs() {
    let mut tabs = Tabs::new()
        .with_tab_id("one", "One")
        .with_tab_id("two", "Two")
        .with_tab_id("three", "Three")
        .with_tab_id("four", "Four");
    let mut rctx = ReactiveCtx::new(NodeId::default());
    assert!(tabs.disable_tab("two", &mut rctx));
    assert!(tabs.hide_tab("three", &mut rctx));
    tabs.set_focus(true);

    let right = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right.clone()), &mut ctx);
    assert!(ctx.handled());
    assert!(tabs.is_active("four"));

    let mut wrap_ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right), &mut wrap_ctx);
    assert!(wrap_ctx.handled());
    assert!(tabs.is_active("one"));
}

#[test]
fn tabs_mouse_click_disabled_tab_does_not_activate() {
    let mut tabs = two_tabs();
    let mut rctx = ReactiveCtx::new(NodeId::default());
    assert!(tabs.disable_tab("two", &mut rctx));
    tabs.on_layout(40, 5);
    let id = NodeId::default();
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
    assert!(tabs.is_active("one"));
}

#[test]
fn tabs_hiding_active_tab_promotes_next_available() {
    let mut tabs = Tabs::new()
        .with_tab_id("one", "One")
        .with_tab_id("two", "Two")
        .with_tab_id("three", "Three");
    let mut rctx = ReactiveCtx::new(NodeId::default());
    tabs.set_active("two", &mut rctx);
    assert!(tabs.is_active("two"));

    assert!(tabs.hide_tab("two", &mut rctx));
    assert!(tabs.is_active("three"));
    assert!(tabs.hide_tab("three", &mut rctx));
    assert!(tabs.is_active("one"));
}
