use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn dock_renders_regions() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 6);
    options.max_width = 12;
    options.max_height = 6;

    let dock = Dock::new()
        .height(6)
        .push_top(Some(1), Label::new("top"))
        .push_bottom(Some(1), Label::new("bottom"))
        .push_left(3, Label::new("L"))
        .push_right(3, Label::new("R"))
        .push_fill(Label::new("center"));

    let buf = FrameBuffer::from_renderable(&console, &options, &dock, None);
    insta::assert_snapshot!(buf.debug_dump());
}
