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
fn preview_root_with_top_and_bottom_renders_sections() {
    let console = Console::new();
    let options = options_for(&console, 40, 8);
    let root = preview_root_with_top_bottom(
        Some("Preview"),
        Some(2),
        Label::new("Top"),
        Label::new("Body"),
        Some(2),
        Label::new("Bottom"),
    );

    let buffer = FrameBuffer::from_renderable(&console, &options, &root, None);
    let lines = buffer.as_plain_lines();
    assert!(lines[0].contains("Preview"));
    assert!(lines.iter().any(|line| line.contains("Top")));
    assert!(lines.iter().any(|line| line.contains("Body")));
    assert!(lines.iter().any(|line| line.contains("Bottom")));
}

#[test]
fn preview_root_without_title_skips_header() {
    let console = Console::new();
    let options = options_for(&console, 24, 3);
    let root = preview_root(None, Label::new("Only body"));

    let buffer = FrameBuffer::from_renderable(&console, &options, &root, None);
    let lines = buffer.as_plain_lines();
    assert!(lines[0].contains("Only body"));
    assert!(lines.iter().all(|line| !line.contains("Textual")));
}
