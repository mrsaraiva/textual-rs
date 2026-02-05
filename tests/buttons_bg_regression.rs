use rich_rs::Console;
use textual::css::{StyleSheet, default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::parse_color_like;
use textual::widgets::WidgetRenderable;

#[test]
fn buttons_demo_default_button_has_background() {
    let css = std::fs::read_to_string("examples/button.tcss").expect("read button.tcss");
    let mut stylesheet = default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(&css));
    let _guard = set_style_context(stylesheet);

    let buttons = Horizontal::new().with_child(
        VerticalScroll::new()
            .with_child(Node::new(Static::new("Standard Buttons")).class("header"))
            .with_child(Button::new("Default"))
            .with_child(Button::primary("Primary!"))
            .with_child(Button::success("Success!"))
            .with_child(Button::warning("Warning!"))
            .with_child(Button::error("Error!")),
    );
    let root = AppRoot::new().with_child(buttons);

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 20);
    options.max_width = 24;
    options.max_height = 20;

    let buf = FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&root), None);
    let lines = buf.as_plain_lines();
    let (y, x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Default").map(|x| (y, x)))
        .expect("expected Default to render");

    let cell = buf.get(x, y);
    let style = cell.style.expect("expected styled cell");
    let expected = parse_color_like("$surface").expect("parse $surface");
    assert_eq!(
        style.bgcolor,
        Some(expected.to_simple_opaque()),
        "expected Default text cell to have bgcolor, got:\n{}",
        buf.debug_dump()
    );
}
