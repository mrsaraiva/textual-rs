use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn scroll_view_offsets_content() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 3);
    options.max_width = 8;
    options.max_height = 3;

    let content = Container::new()
        .with_child(Label::new("line1"))
        .with_child(Label::new("line2"))
        .with_child(Label::new("line3"))
        .with_child(Label::new("line4"));

    let mut scroll = ScrollView::new(content).height(2);
    scroll.scroll_to(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    insta::assert_snapshot!(buf.debug_dump());
}
