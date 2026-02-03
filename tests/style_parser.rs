use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::widget::set_style_context;

#[test]
fn stylesheet_parser_applies_rules() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let css = r#"
Label { fg: red; bold: true; }
#hero { underline: true; }
.notice { bg: blue; }
"#;

    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Node::new(Label::new("hi")).id("hero").class("notice");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
    assert_eq!(style.color, Some(Color::Standard(1)));
    assert_eq!(style.bgcolor, Some(Color::Standard(4)));
}
