use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn overlay_shows_modal_over_base() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let base = Label::new("base content");
    let modal = Frame::new(Label::new("modal"));
    let overlay = Overlay::new(base, modal);

    let buf = FrameBuffer::from_renderable(&console, &options, &overlay, None);
    insta::assert_snapshot!(buf.debug_dump());
}
