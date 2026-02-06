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
fn key_panel_renders_bindings() {
    let console = Console::new();
    let options = options_for(&console, 32, 4);
    let panel = KeyPanel::new().with_bindings(vec![
        FooterBinding::new("^q", "Quit application"),
        FooterBinding::new("⇥", "Focus next"),
    ]);

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].contains("Key Bindings"));
    assert!(lines.iter().any(|line| line.contains("^q")));
    assert!(lines.iter().any(|line| line.contains("Quit application")));
}

#[test]
fn key_panel_scrolls_with_actions() {
    let console = Console::new();
    let options = options_for(&console, 36, 5);
    let mut panel = KeyPanel::new().with_bindings(vec![
        FooterBinding::new("a", "one"),
        FooterBinding::new("b", "two"),
        FooterBinding::new("c", "three"),
        FooterBinding::new("d", "four"),
    ]);

    let before = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let before_lines = before.as_plain_lines();
    assert!(before_lines.iter().all(|line| !line.contains("four")));

    panel.on_event(&Event::Action(Action::ScrollDown), &mut EventCtx::default());
    panel.on_event(&Event::Action(Action::ScrollDown), &mut EventCtx::default());
    panel.on_event(&Event::Action(Action::ScrollDown), &mut EventCtx::default());

    let after = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let after_lines = after.as_plain_lines();
    assert!(after_lines.iter().any(|line| line.contains("four")));
}

#[test]
fn key_panel_updates_on_bindings_changed_event() {
    let console = Console::new();
    let options = options_for(&console, 36, 4);
    let mut panel = KeyPanel::new().with_bindings(vec![FooterBinding::new("a", "alpha")]);

    panel.on_event(
        &Event::BindingsChanged(vec![BindingHint {
            key: "x, y".to_string(),
            description: "Updated action".to_string(),
        }]),
        &mut EventCtx::default(),
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("x, y")));
    assert!(lines.iter().any(|line| line.contains("Updated action")));
}
