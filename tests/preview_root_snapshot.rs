use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn preview_root_top_bottom_snapshot() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (48, 10);
    options.max_width = 48;
    options.max_height = 10;

    let root = preview_root_with_top_bottom(
        Some("Preview"),
        Some(2),
        Label::new("Top panel"),
        Label::new("Main body"),
        Some(2),
        Label::new("Bottom panel"),
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &root, None);
    insta::assert_snapshot!(buf.debug_dump());
}
