use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn markdown_renders_basic_blocks() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (26, 6);
    options.max_width = 26;
    options.max_height = 6;

    let markdown = Markdown::new("# Title\n\n- Alpha\n- Beta\n\n`code`");

    let buf = FrameBuffer::from_renderable(&console, &options, &markdown, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn markdown_h1_uses_default_component_style() {
    let _guard = set_style_context(default_widget_stylesheet());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 3);
    options.max_width = 24;
    options.max_height = 3;

    let mut markdown = Markdown::new("# Heading");
    markdown.on_layout(24, 3);

    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&markdown), None);

    let mut heading_cell = None;
    for x in 0..buf.width {
        let cell = buf.get(x, 0);
        if cell.text == "H" {
            heading_cell = Some(cell.clone());
            break;
        }
    }
    let heading_cell = heading_cell.expect("expected heading glyph in first line");
    let style = heading_cell.style.expect("expected heading style");
    assert!(style.color.is_some());
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
}
