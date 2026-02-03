use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn debug_layout_draws_borders_and_sizes() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 7);
    options.max_width = 12;
    options.max_height = 7;

    let container = Container::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("beta"));

    let debug = DebugLayout::enabled();
    let segments = container.render_with_debug(&console, &options, &debug);
    let lines = rich_rs::Segment::split_and_crop_lines(segments, 12, None, true, false);
    let buf = FrameBuffer::from_lines(&lines, 12, 7, None);

    insta::assert_snapshot!(buf.debug_dump());
}
