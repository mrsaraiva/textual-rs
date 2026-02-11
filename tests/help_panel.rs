use rich_rs::Console;
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
    assert!(lines.iter().any(|line| line.contains("Keys")));
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
    assert!(inactive_lines.iter().any(|line| line.contains("Keys")));

    panel.on_event(&Event::AppFocus(true), &mut EventCtx::default());
    let active = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let active_lines = active.as_plain_lines();
    assert!(active_lines.iter().any(|line| line.contains("Widget help")));
}

#[test]
fn help_panel_show_help_class_tracks_app_focus_state() {
    let mut panel = HelpPanel::new().with_help("## Widget help");
    assert!(
        panel
            .style_classes()
            .iter()
            .any(|class| class == "-show-help")
    );

    panel.on_event(&Event::AppFocus(false), &mut EventCtx::default());
    assert!(
        !panel
            .style_classes()
            .iter()
            .any(|class| class == "-show-help")
    );

    panel.on_event(&Event::AppFocus(true), &mut EventCtx::default());
    assert!(
        panel
            .style_classes()
            .iter()
            .any(|class| class == "-show-help")
    );
}

#[test]
fn help_panel_help_can_be_driven_via_messages() {
    let mut panel = HelpPanel::new();
    let mut ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::HelpPanelSetHelp {
                panel: panel.id(),
                markup: "## Runtime help".to_string(),
            },
        },
        &mut ctx,
    );

    assert!(ctx.handled());
    assert!(panel.showing_help());
    assert_eq!(panel.help(), "## Runtime help");

    let mut clear_ctx = EventCtx::default();
    panel.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::HelpPanelClearHelp { panel: panel.id() },
        },
        &mut clear_ctx,
    );
    assert!(clear_ctx.handled());
    assert!(!panel.showing_help());
}
