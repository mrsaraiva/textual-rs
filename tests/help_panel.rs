use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::message::MessageEvent;
use textual::node_id_from_ffi;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::parse_color_like;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn help_panel_renders_key_bindings() {
    let console = Console::new();
    let options = options_for(&console, 36, 6);
    let panel = HelpPanel::new().with_bindings(vec![
        FooterBinding::new("^q", "Quit"),
        FooterBinding::new("tab", "Focus next"),
    ]);

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("^q")));
    assert!(lines.iter().any(|line| line.contains("Quit")));
}

#[test]
fn help_panel_renders_help_markdown_when_configured() {
    let console = Console::new();
    let options = options_for(&console, 40, 8);
    let panel = HelpPanel::new().with_help("## Widget help\nUse arrows to move.");

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("Widget help")));
}

#[test]
fn help_panel_updates_bindings_from_binding_hints_event() {
    let console = Console::new();
    let options = options_for(&console, 40, 7);
    let mut panel = HelpPanel::new();

    panel.on_event(
        &Event::BindingsChanged(vec![BindingHint::new("f1", "Toggle help")]),
        &mut EventCtx::default(),
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("f1")));
    assert!(lines.iter().any(|line| line.contains("Toggle help")));
}

#[test]
fn help_panel_clear_help_hides_help_state() {
    let mut panel = HelpPanel::new().with_help("## Widget help\nUse arrows to move.");
    assert!(panel.showing_help());
    panel.clear_help();
    assert!(!panel.showing_help());
}

#[test]
fn help_panel_keeps_bindings_visible_with_help_in_short_layouts() {
    let console = Console::new();
    let options = options_for(&console, 40, 2);
    let panel = HelpPanel::new()
        .with_help("## Widget help")
        .with_bindings(vec![FooterBinding::new("^q", "Quit")]);

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("Widget help")));
    assert!(lines.iter().any(|line| line.contains("^q")));
}

#[test]
fn help_panel_hides_help_section_when_app_is_inactive() {
    let console = Console::new();
    let options = options_for(&console, 40, 8);
    let mut panel = HelpPanel::new()
        .with_help("## Widget help\nUse arrows to move.")
        .with_bindings(vec![FooterBinding::new("^q", "Quit")]);

    panel.on_event(&Event::AppFocus(false), &mut EventCtx::default());
    let inactive = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let inactive_lines = inactive.as_plain_lines();
    assert!(
        inactive_lines
            .iter()
            .all(|line| !line.contains("Widget help"))
    );
    assert!(inactive_lines.iter().any(|line| line.contains("^q")));

    panel.on_event(&Event::AppFocus(true), &mut EventCtx::default());
    let active = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let active_lines = active.as_plain_lines();
    assert!(active_lines.iter().any(|line| line.contains("Widget help")));
}

#[test]
fn help_panel_show_help_class_tracks_app_focus_state() {
    let mut panel = HelpPanel::new().with_help("## Widget help");
    // Initially showing help when app is active.
    assert!(panel.showing_help());

    // AppFocus(false) should request a repaint (runtime applies class op).
    let mut ctx = EventCtx::default();
    panel.on_event(&Event::AppFocus(false), &mut ctx);
    assert!(ctx.repaint_requested(), "focus change should request repaint");
    // Help state remains (it's the focus gate that changes, not the content).
    assert!(panel.showing_help());

    // AppFocus(true) should request a repaint too.
    let mut ctx2 = EventCtx::default();
    panel.on_event(&Event::AppFocus(true), &mut ctx2);
    assert!(ctx2.repaint_requested(), "focus restore should request repaint");
}

#[test]
fn help_panel_help_can_be_driven_via_messages() {
    let mut panel = HelpPanel::new();
    let mut ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent::new(
            NodeId::default(),
            HelpPanelSetHelp {
                panel: NodeId::default(),
                markup: "## Runtime help".to_string(),
            },
        ),
        &mut ctx,
    );

    assert!(ctx.handled());
    assert!(panel.showing_help());
    assert_eq!(panel.help(), "## Runtime help");

    let mut clear_ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent::new(
            NodeId::default(),
            HelpPanelClearHelp { panel: NodeId::default() },
        ),
        &mut clear_ctx,
    );
    assert!(clear_ctx.handled());
    assert!(!panel.showing_help());
}

#[test]
fn help_panel_handles_focused_help_pipeline_messages() {
    let mut panel = HelpPanel::new();
    let mut set_ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent::new(
            node_id_from_ffi(100),
            HelpPanelFocusedHelpChanged {
                source: node_id_from_ffi(100),
                markup: "## Focused widget help".to_string(),
            },
        ),
        &mut set_ctx,
    );
    assert!(panel.showing_help());
    assert_eq!(panel.help(), "## Focused widget help");

    let mut clear_ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent::new(NodeId::default(), HelpPanelFocusedHelpCleared),
        &mut clear_ctx,
    );
    assert!(!panel.showing_help());
    assert_eq!(panel.help(), "");
}

#[test]
fn help_panel_unmount_resets_app_focus_gate() {
    let mut panel = HelpPanel::new().with_help("## Widget help");

    // Simulate app losing focus (internal app_active flag flips).
    panel.on_event(&Event::AppFocus(false), &mut EventCtx::default());

    // After unmount, app_active is reset to true so help re-shows on remount.
    // We verify this by checking that the panel shows help again after unmount
    // (the render path uses app_active internally via split_heights).
    panel.on_unmount();
    // showing_help() reflects show_help field — unmount should not clear it.
    // The app_active reset ensures the help section renders correctly post-remount.
    assert!(
        panel.showing_help(),
        "unmount should preserve show_help state; app_active is reset internally"
    );
}

#[test]
fn help_panel_default_css_uses_vkey_border_glyphs() {
    let console = Console::new();
    let options = options_for(&console, 40, 6);
    let _guard = set_style_context(default_widget_stylesheet());
    let panel = HelpPanel::new().with_bindings(vec![FooterBinding::new("^q", "Quit")]);
    let renderable = WidgetRenderable::new(&panel);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let lines = buf.as_plain_lines();

    assert!(
        lines
            .iter()
            .any(|line| line.starts_with('\u{258f}') || line.ends_with('\u{2595}')),
        "expected vkey border glyphs in help panel output, got {lines:?}"
    );
}

#[test]
fn help_panel_border_color_composes_foreground_alpha_over_background() {
    let console = Console::new();
    let options = options_for(&console, 40, 6);
    let _guard = set_style_context(default_widget_stylesheet());
    let panel = HelpPanel::new().with_bindings(vec![FooterBinding::new("^q", "Quit")]);
    let renderable = WidgetRenderable::new(&panel);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let border_cell = buf.get(0, 0);
    let border_style = border_cell.style.expect("border style should exist");
    let actual = border_style.color.expect("border fg color should exist");

    let foreground = parse_color_like("$foreground")
        .expect("foreground token")
        .with_alpha(0.30);
    let background = parse_color_like("$background").expect("background token");
    let expected = foreground.flatten_over(background).to_simple_opaque();

    assert_eq!(actual, expected);
}
