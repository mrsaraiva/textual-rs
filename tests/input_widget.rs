use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn input_accepts_typing() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 1);
    options.max_width = 12;
    options.max_height = 1;

    let mut input = Input::new().with_placeholder("name");
    input.set_focus(true);

    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    input.on_event(&Event::Key(key), &mut EventCtx::default());

    let buf = FrameBuffer::from_renderable(&console, &options, &input, None);
    insta::assert_snapshot!(buf.debug_dump());
}
