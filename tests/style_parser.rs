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
    let red = Color::parse("red").expect("parse red");
    let blue = Color::parse("blue").expect("parse blue");
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
    assert_eq!(style.color, Some(red.to_simple_opaque()));
    assert_eq!(style.bgcolor, Some(blue.to_simple_opaque()));
}

#[test]
fn rgba_background_is_composited_over_base_background() {
    use rich_rs::Console;
    use textual::render::FrameBuffer;
    use textual::style::parse_color_like;
    use textual::widget::set_style_context;

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (1, 1);
    options.max_width = 1;
    options.max_height = 1;

    let css = r#"
Label { bg: rgba(255,0,0,0.5); }
"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Label::new("x");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    let base = parse_color_like("$background").expect("parse $background");
    let expected = Color::rgba(255, 0, 0, 128).flatten_over(base);
    assert_eq!(style.bgcolor, Some(expected.to_simple_opaque()));
}
