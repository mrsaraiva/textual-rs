use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn row_splits_width_across_children() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 2);
    options.max_width = 10;
    options.max_height = 2;

    let row = Row::new()
        .with_child(Label::new("left"))
        .with_child(Label::new("right"));
    let buf = FrameBuffer::from_renderable(&console, &options, &row, None);

    insta::assert_snapshot!(buf.debug_dump());
}
