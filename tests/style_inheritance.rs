use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn inherited_styles_apply_to_children() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 1);
    options.max_width = 10;
    options.max_height = 1;

    let row = Row::new().with_child(Label::new("hello"));
    let green = Color::parse("green").expect("parse green");
    let styled = Styled::new(row, Style::new().fg(green));
    let renderable = WidgetRenderable::new(&styled);

    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.color, Some(green.to_simple_opaque()));
}
