use rich_rs::Console;
use textual::prelude::*;
use textual::css::{StyleSheet, set_style_context};
use textual::widgets::WidgetRenderable;

#[test]
fn buttons_demo_renders_labels() {
    let css = std::fs::read_to_string("examples/button.tcss").expect("read button.tcss");
    let mut stylesheet = textual::css::default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(&css));
    let _guard = set_style_context(stylesheet);

    let buttons = Horizontal::new()
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Standard Buttons")).class("header"))
                .with_child(Button::new("Default"))
                .with_child(Button::primary("Primary!"))
                .with_child(Button::success("Success!"))
                .with_child(Button::warning("Warning!"))
                .with_child(Button::error("Error!")),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true))
                .with_child(Button::primary("Primary!").disabled(true))
                .with_child(Button::success("Success!").disabled(true))
                .with_child(Button::warning("Warning!").disabled(true))
                .with_child(Button::error("Error!").disabled(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Flat Buttons")).class("header"))
                .with_child(Button::new("Default").flat(true))
                .with_child(Button::primary("Primary!").flat(true))
                .with_child(Button::success("Success!").flat(true))
                .with_child(Button::warning("Warning!").flat(true))
                .with_child(Button::error("Error!").flat(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Flat Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true).flat(true))
                .with_child(Button::primary("Primary!").disabled(true).flat(true))
                .with_child(Button::success("Success!").disabled(true).flat(true))
                .with_child(Button::warning("Warning!").disabled(true).flat(true))
                .with_child(Button::error("Error!").disabled(true).flat(true)),
        );

    let root = AppRoot::new().with_child(buttons);
    let scroll_root = ScrollView::new(root);

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (120, 30);
    options.max_width = 120;
    options.max_height = 30;

    let buf = textual::render::FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&scroll_root),
        None,
    );
    let plain = buf.as_plain_lines().join("\n");
    assert!(
        plain.contains("Primary!"),
        "expected Primary! in rendered output, got:\n{}",
        buf.debug_dump()
    );
}
