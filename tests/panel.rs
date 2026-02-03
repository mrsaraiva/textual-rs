use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn panel_renders_border_and_title() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (14, 4);
    options.max_width = 14;
    options.max_height = 4;

    let panel = Panel::new(Label::new("hello")).title("Title").padding(1);
    let buf = FrameBuffer::from_renderable(&console, &options, &panel, None);
    insta::assert_snapshot!(buf.debug_dump());
}
