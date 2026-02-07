use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

fn render_buffer(palette: &CommandPalette) -> FrameBuffer {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (72, 14);
    options.max_width = 72;
    options.max_height = 14;
    FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(palette), None)
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
