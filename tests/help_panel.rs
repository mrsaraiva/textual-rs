use rich_rs::Console;
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
