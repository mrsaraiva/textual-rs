use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn tabs_render_header_and_active_content() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (20, 3);
    options.max_width = 20;
    options.max_height = 3;

    let tabs = Tabs::new()
        .with_tab("One", Label::new("first"))
        .with_tab("Two", Label::new("second"));

    let buf = FrameBuffer::from_renderable(&console, &options, &tabs, None);
    insta::assert_snapshot!(buf.debug_dump());
}
