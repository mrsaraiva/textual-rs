use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::widget::set_style_context;

#[test]
fn stylesheet_applies_type_and_id_styles() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let label = Node::new(Label::new("hi")).id("hero");

    let mut sheet = StyleSheet::new();
    sheet.add_type("Label", Style::new().bold(true));
    sheet.add_id("hero", Style::new().underline(true));

    let _guard = set_style_context(sheet);
    let buf = FrameBuffer::from_renderable(&console, &options, &label, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style to be set");
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
}
