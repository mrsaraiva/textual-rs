use rich_rs::Console;
use textual::action::ParsedAction;
use textual::css::{
    AppRuntimePseudos, default_widget_stylesheet, set_app_runtime_pseudos, set_style_context,
};
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};
use textual::style::parse_color_like;

fn render_tabbed_frame(tabs: &mut TabbedContent, width: u16, height: u16) -> FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(tabs).expect("tree should exist");
    render_tree_to_frame(&mut tree, tabs, &console, width as usize, height as usize)
}

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

#[test]
fn tabbed_content_exposes_switch_tab_binding_hint_when_multiple_panes() {
    let tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    let hints = tabs.binding_hints();
    assert_eq!(
        hints,
        vec![
            BindingHint::new("left", "Previous tab")
                .with_key_display("←")
                .hidden(true),
            BindingHint::new("right", "Next tab")
                .with_key_display("→")
                .hidden(true),
        ]
    );
}

#[test]
fn tabbed_content_hides_switch_tab_binding_hint_for_single_pane() {
    let tabs = TabbedContent::new().with_pane(TabPane::new("One", Label::new("first")).id("one"));
    assert!(tabs.binding_hints().is_empty());
}

#[test]
fn tabbed_content_default_css_focus_styles_active_tab_and_underline() {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 2);
    options.max_width = 24;
    options.max_height = 2;

    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    tabs.set_focus(true);
    tabs.on_layout(24, 2);

    let buf = render_tabbed_frame(&mut tabs, 24, 2);

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
                .flatten_over(
                    parse_color_like("$block-cursor-background").expect("block cursor background"),
                )
                .to_simple_opaque()
        )
    );
    assert_eq!(active_tab_style.bold, Some(true));

    let active_underline_style = buf.get(1, 1).style.expect("active underline style");
    let inactive_underline_style = buf.get(16, 1).style.expect("inactive underline style");
    let active_underline_fg = parse_color_like("$block-cursor-background")
        .expect("active underline foreground")
        .to_simple_opaque();
    assert_eq!(active_underline_style.color, Some(active_underline_fg));
    assert_ne!(active_underline_style.color, inactive_underline_style.color);
}

#[test]
fn tabbed_content_default_css_styles_inactive_tab_differently_from_active_tab() {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 2);
    options.max_width = 24;
    options.max_height = 2;

    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    let buf = render_tabbed_frame(&mut tabs, 24, 2);

    let active_style = buf.get(1, 0).style.expect("active style");
    let inactive_style = buf.get(7, 0).style.expect("inactive style");
    assert_ne!(active_style.color, inactive_style.color);
}

#[test]
fn tabbed_content_ansi_uses_bright_blue_underline_and_no_active_tab_bg() {
    let _guard = set_style_context(default_widget_stylesheet());
    let _pseudos = set_app_runtime_pseudos(AppRuntimePseudos {
        ansi: true,
        ..Default::default()
    });
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 2);
    options.max_width = 24;
    options.max_height = 2;

    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    tabs.set_focus(true);
    tabs.on_layout(24, 2);

    let buf = render_tabbed_frame(&mut tabs, 24, 2);
    let active_tab_style = buf.get(1, 0).style.expect("active tab style");
    let active_underline_style = buf.get(1, 1).style.expect("active underline style");

    let block_cursor_bg = parse_color_like("$block-cursor-background")
        .expect("block cursor background")
        .to_simple_opaque();
    assert_ne!(active_tab_style.bgcolor, Some(block_cursor_bg));
    assert_eq!(
        active_underline_style.color,
        Some(
            parse_color_like("ansi_bright_blue")
                .expect("ansi bright blue")
                .to_simple_opaque()
        )
    );
}

#[test]
fn tabbed_content_keyboard_navigation_skips_disabled_and_hidden_panes() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"))
        .with_pane(TabPane::new("Three", Label::new("third")).id("three"))
        .with_pane(TabPane::new("Four", Label::new("fourth")).id("four"));
    assert!(tabs.disable_pane("two"));
    assert!(tabs.hide_pane("three"));
    tabs.set_focus(true);

    let right = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right.clone()), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(tabs.active_id(), Some("four"));

    let mut wrap_ctx = EventCtx::default();
    tabs.on_event(&Event::Key(right), &mut wrap_ctx);
    assert!(wrap_ctx.handled());
    assert_eq!(tabs.active_id(), Some("one"));
}

#[test]
fn tabbed_content_set_active_id_rejects_disabled_or_hidden_panes() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"))
        .with_pane(TabPane::new("Three", Label::new("third")).id("three"));
    assert!(tabs.disable_pane("two"));
    assert!(tabs.hide_pane("three"));

    assert!(!tabs.set_active_id("two", None));
    assert!(!tabs.set_active_id("three", None));
    assert_eq!(tabs.active_id(), Some("one"));
}

#[test]
fn tabbed_content_hiding_active_pane_promotes_next_available() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"))
        .with_pane(TabPane::new("Three", Label::new("third")).id("three"));
    assert!(tabs.set_active_id("two", None));
    assert_eq!(tabs.active_id(), Some("two"));

    assert!(tabs.hide_pane("two"));
    assert_eq!(tabs.active_id(), Some("three"));
    assert!(tabs.hide_pane("three"));
    assert_eq!(tabs.active_id(), Some("one"));
}

#[test]
fn tabbed_content_mouse_click_disabled_pane_tab_does_not_activate() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    assert!(tabs.disable_pane("two"));
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
    assert_eq!(tabs.active_id(), Some("one"));
}

#[test]
fn tabbed_content_show_tab_action_switches_active_pane() {
    let mut tabs = TabbedContent::new()
        .with_pane(TabPane::new("One", Label::new("first")).id("one"))
        .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
    let action = ParsedAction {
        namespace: None,
        name: "show_tab".to_string(),
        arguments: vec!["two".to_string()],
    };
    let mut ctx = EventCtx::default();
    assert!(tabs.execute_action(&action, &mut ctx));
    assert!(ctx.handled());
    assert!(ctx.repaint_requested());
    assert_eq!(tabs.active_id(), Some("two"));
}
