use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn container_clips_to_viewport_height() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 3);
    options.max_width = 10;
    options.max_height = 3;

    let mut root = Container::new();
    root.push(Label::new("line1"));
    root.push(Label::new("line2"));
    root.push(Label::new("line3"));
    root.push(Label::new("line4"));

    let buf = FrameBuffer::from_renderable(&console, &options, &root, None);
    insta::assert_snapshot!(buf.debug_dump());
}
