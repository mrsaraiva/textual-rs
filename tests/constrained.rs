use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn constrained_limits_child_height() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 5);
    options.max_width = 12;
    options.max_height = 5;

    let list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    let constrained = Constrained::new(list).max_height(2);
    let root = Container::new().with_child(constrained);

    let buf = FrameBuffer::from_renderable(&console, &options, &root, None);
    insta::assert_snapshot!(buf.debug_dump());
}
