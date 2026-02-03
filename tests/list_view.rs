use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn list_view_renders_selection() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let mut list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    list.set_focus(true);
    list.set_selected(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &list, None);
    insta::assert_snapshot!(buf.debug_dump());
}
