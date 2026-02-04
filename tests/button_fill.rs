use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::widget::{default_widget_stylesheet, set_style_context};

#[test]
fn button_middle_row_is_painted_with_bg() {
    let mut sheet = default_widget_stylesheet();
    // Match the demo: margins are applied by user CSS.
    sheet.extend(&StyleSheet::parse("Button { margin: 1 2; }"));
    let _guard = set_style_context(sheet);

    let button = Button::new("Default");

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 3);
    options.max_width = 24;
    options.max_height = 3;

    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&button), None);

    // Middle row, left edge should have a background color applied.
    let cell = buf.get(0, 1);
    let style = cell.style.expect("expected painted cell style");
    assert!(
        style.bgcolor.is_some(),
        "expected bgcolor on middle row cell, got:\n{}",
        buf.debug_dump()
    );
}
