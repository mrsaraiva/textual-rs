use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn checkbox_toggles() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 1);
    options.max_width = 16;
    options.max_height = 1;

    let mut checkbox = Checkbox::new("remember me");
    checkbox.set_focus(true);

    let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
    checkbox.on_event(&Event::Key(key), &mut EventCtx::default());

    let buf = FrameBuffer::from_renderable(&console, &options, &checkbox, None);
    insta::assert_snapshot!(buf.debug_dump());
}
