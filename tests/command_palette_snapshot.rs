use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

fn render_buffer_with_size(palette: &CommandPalette, width: usize, height: usize) -> FrameBuffer {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(palette), None)
}

fn render_buffer(palette: &CommandPalette) -> FrameBuffer {
    render_buffer_with_size(palette, 72, 14)
}

#[test]
fn command_palette_closed_snapshot() {
    let palette = CommandPalette::new(Label::new("Body content"));
    let buf = render_buffer(&palette);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn command_palette_open_snapshot() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn command_palette_renders_markup_commands_without_literal_tags() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.set_commands(vec![PaletteCommand::new(
        "markup",
        "[bold]Deploy[/]",
        "[green]Ship[/] current build",
    )]);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("Deploy")));
    assert!(lines.iter().any(|line| line.contains("Ship current build")));
    assert!(!lines.iter().any(|line| line.contains("[bold]")));
    assert!(!lines.iter().any(|line| line.contains("[green]")));
}

#[test]
fn command_palette_open_render_handles_small_viewport() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.on_layout(40, 4);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer_with_size(&palette, 40, 4);
    let lines = buf.as_plain_lines();
    assert_eq!(lines.len(), 4);
    assert!(lines[2].contains("Search for commands"));
}
