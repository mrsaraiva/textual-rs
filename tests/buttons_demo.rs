// `Node` is deprecated (RA2.6) but intentionally exercised here until the 1.x
// container seed-builder unification migrates these off the wrapper.
#![allow(deprecated)]
use rich_rs::Console;
use textual::css::{StyleSheet, set_style_context};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

fn load_button_css() -> String {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let css_path = repo_root.join("docs/examples/widgets/examples/shared/button.tcss");
    std::fs::read_to_string(css_path).expect("read button.tcss")
}

fn render_tree(root: &mut dyn Widget, width: usize, height: usize) -> textual::render::FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    render_tree_to_frame(&mut tree, root, &console, width, height)
}

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
    let css = load_button_css();
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

    let mut scroll_root = ScrollView::new(AppRoot::new().with_child(buttons));
    let buf = render_tree(&mut scroll_root, 120, 30);
    let plain = buf.as_plain_lines().join("\n");
    assert!(
        plain.contains("Primary!"),
        "expected Primary! in rendered output, got:\n{}",
        buf.debug_dump()
    );
}

#[test]
fn buttons_demo_header_renders_with_button_tcss_loaded() {
    let css = load_button_css();
    let mut stylesheet = textual::css::default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(&css));
    let _guard = set_style_context(stylesheet);

    let mut root = AppRoot::new().with_child(Horizontal::new().with_child(
        VerticalScroll::new().with_child(Static::new("Standard Buttons").class("header")),
    ));
    let buf = render_tree(&mut root, 40, 5);
    let plain = buf.as_plain_lines().join("\n");
    let _position = plain
        .lines()
        .enumerate()
        .find_map(|(y, line)| line.find("Standard").map(|x| (y, x)))
        .expect("header text not found");
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

    let mut root = Row::new()
        .with_child(Button::primary("Primary!"))
        .with_child(Button::primary("Primary!").disabled(true));
    let buf = render_tree(&mut root, 40, 5);
    let plain = buf.as_plain_lines().join("\n");

    let mut labels: Vec<(usize, usize)> = Vec::new();
    for (row, row_text) in plain.lines().enumerate() {
        let mut search_from = 0usize;
        while let Some(offset) = row_text[search_from..].find("Primary!") {
            let x = search_from + offset;
            labels.push((x, row));
            search_from = x + "Primary!".len();
        }
    }
    assert!(
        labels.len() >= 2,
        "expected at least two Primary! labels, got:\n{}",
        buf.debug_dump()
    );
    labels.sort_by_key(|(x, y)| (*y, *x));
    let (first_x, first_y) = labels[0];
    let (second_x, second_y) = labels[1];

    let enabled_cell = buf.get(first_x, first_y);
    let disabled_cell = buf.get(second_x, second_y);
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
