use rich_rs::Console;
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
