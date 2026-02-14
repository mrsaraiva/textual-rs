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

#[test]
fn transparent_child_composes_parent_background() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 1);
    options.max_width = 10;
    options.max_height = 1;

    let row = Row::new().with_child(Label::new("hello"));
    let blue = Color::parse("blue").expect("parse blue");
    let styled = Styled::new(row, Style::new().bg(blue));
    let renderable = WidgetRenderable::new(&styled);

    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let cell = buf.get(0, 0);
    assert_eq!(
        cell.style.and_then(|s| s.bgcolor),
        Some(blue.to_simple_opaque()),
        "transparent child should compose onto parent background in final output"
    );
}

#[test]
fn child_background_overrides_parent_background() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 1);
    options.max_width = 10;
    options.max_height = 1;

    let green = Color::parse("green").expect("parse green");
    let blue = Color::parse("blue").expect("parse blue");

    let child = Styled::new(Label::new("hello"), Style::new().bg(green));
    let row = Row::new().with_child(child);
    let styled = Styled::new(row, Style::new().bg(blue));
    let renderable = WidgetRenderable::new(&styled);

    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let cell = buf.get(0, 0);
    assert_eq!(
        cell.style.and_then(|s| s.bgcolor),
        Some(green.to_simple_opaque()),
        "child explicit background should win over parent background"
    );
}
