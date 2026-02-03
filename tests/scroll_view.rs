use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn scroll_view_renders_offset_viewport() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (14, 3);
    options.max_width = 14;
    options.max_height = 3;

    let list = ListView::new(vec![
        "item 1".to_string(),
        "item 2".to_string(),
        "item 3".to_string(),
        "item 4".to_string(),
    ]);
    let mut scroll = ScrollView::new(list).height(3);
    scroll.scroll_to(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    insta::assert_snapshot!(buf.debug_dump());
}
