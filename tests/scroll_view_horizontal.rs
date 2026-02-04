use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn scroll_view_renders_horizontal_offset() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 2);
    options.max_width = 8;
    options.max_height = 2;

    let rows = ListView::new(vec!["alpha-bravo".to_string(), "charlie-delta".to_string()]);
    let mut scroll = ScrollView::new(rows).height(2);
    scroll.scroll_to_x(6);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    insta::assert_snapshot!(buf.debug_dump());
}
