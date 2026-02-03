use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn frame_renders_border_and_padding() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 5);
    options.max_width = 16;
    options.max_height = 5;

    let frame = Frame::new(Button::new("OK")).padding(1);
    let buf = FrameBuffer::from_renderable(&console, &options, &frame, None);

    insta::assert_snapshot!(buf.debug_dump());
}
