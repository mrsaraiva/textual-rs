use rich_rs::Console;
use textual::css::{StyleSheet, set_style_context};
use textual::prelude::*;
use textual::widgets::WidgetRenderable;

fn simple_color_rgb(color: rich_rs::SimpleColor) -> (u8, u8, u8) {
    match color {
        rich_rs::SimpleColor::Rgb { r, g, b } => (r, g, b),
        other => {
            let hex = other.get_hex();
            match rich_rs::SimpleColor::parse(&hex).expect("parse palette color hex") {
                rich_rs::SimpleColor::Rgb { r, g, b } => (r, g, b),
                _ => (255, 255, 255),
            }
        }
    }
}

fn color_distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let dr = a.0 as i32 - b.0 as i32;
    let dg = a.1 as i32 - b.1 as i32;
    let db = a.2 as i32 - b.2 as i32;
    dr * dr + dg * dg + db * db
}

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

#[test]
fn buttons_demo_headers_are_bold_from_button_tcss() {
    let css = std::fs::read_to_string("examples/button.tcss").expect("read button.tcss");
    let mut stylesheet = textual::css::default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(&css));
    let _guard = set_style_context(stylesheet);

    let root = AppRoot::new().with_child(
        Horizontal::new().with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Standard Buttons")).class("header")),
        ),
    );
    let renderable = WidgetRenderable::new(&root);
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (40, 5);
    options.max_width = 40;
    options.max_height = 5;
    let buf = textual::render::FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let plain = buf.as_plain_lines().join("\n");
    let (y, x) = plain
        .lines()
        .enumerate()
        .find_map(|(y, line)| line.find("Standard").map(|x| (y, x)))
        .expect("header text not found");
    let style = buf
        .get(x, y)
        .style
        .expect("header cell should have style information");
    assert_eq!(style.bold, Some(true), "header text should be bold");
}

#[test]
fn disabled_non_flat_primary_text_is_dimmer_than_enabled() {
    let mut stylesheet = textual::css::default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(
        r#"
        Row {
            width: auto;
            height: auto;
        }
        Button {
            margin: 0 1;
        }
    "#,
    ));
    let _guard = set_style_context(stylesheet);

    let root = AppRoot::new().with_child(
        Row::new()
            .with_child(Button::primary("Primary!"))
            .with_child(Button::primary("Primary!").disabled(true)),
    );
    let renderable = WidgetRenderable::new(&root);
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (40, 5);
    options.max_width = 40;
    options.max_height = 5;
    let buf = textual::render::FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let plain = buf.as_plain_lines().join("\n");

    let line = plain.lines().find(|line| line.contains("Primary!")).expect("line");
    let first = line.find("Primary!").expect("first primary");
    let second = line[first + "Primary!".len()..]
        .find("Primary!")
        .map(|offset| first + "Primary!".len() + offset)
        .expect("second primary");
    let y = plain
        .lines()
        .enumerate()
        .find_map(|(row, row_text)| row_text.contains("Primary!").then_some(row))
        .expect("row for primary labels");

    let enabled_cell = buf.get(first, y);
    let disabled_cell = buf.get(second, y);
    let enabled_style = enabled_cell.style.expect("enabled button style");
    let disabled_style = disabled_cell.style.expect("disabled button style");
    let enabled_fg = simple_color_rgb(enabled_style.color.expect("enabled fg"));
    let disabled_fg = simple_color_rgb(disabled_style.color.expect("disabled fg"));
    let bg = simple_color_rgb(disabled_style.bgcolor.expect("disabled bg"));

    assert_ne!(
        enabled_fg, disabled_fg,
        "disabled primary text should not match enabled text color"
    );
    assert!(
        color_distance(disabled_fg, bg) < color_distance(enabled_fg, bg),
        "disabled primary text should be closer to button background (dimmer)"
    );
}
