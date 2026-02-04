use std::env;
use std::path::PathBuf;

use rich_rs::Console;
use textual::prelude::*;
use textual::widget::{StyleSheet, WidgetRenderable, default_widget_stylesheet, set_style_context};

fn build_button_demo() -> ScrollView {
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
    ScrollView::new(root).scroll_step(2)
}

fn parse_args() -> (String, usize, usize, PathBuf, String) {
    let mut demo = "button".to_string();
    let mut width = 80usize;
    let mut height = 24usize;
    let mut out = PathBuf::from("demo_rust.svg");
    let mut title = "textual-rs demo".to_string();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => {
                if let Some(value) = args.next() {
                    demo = value;
                }
            }
            "--width" => {
                if let Some(value) = args.next() {
                    width = value.parse().unwrap_or(width);
                }
            }
            "--height" => {
                if let Some(value) = args.next() {
                    height = value.parse().unwrap_or(height);
                }
            }
            "--out" => {
                if let Some(value) = args.next() {
                    out = PathBuf::from(value);
                }
            }
            "--title" => {
                if let Some(value) = args.next() {
                    title = value;
                }
            }
            _ => {}
        }
    }

    (demo, width, height, out, title)
}

fn main() -> Result<()> {
    let (demo, width, height, out, title) = parse_args();

    let mut stylesheet = default_widget_stylesheet();
    match demo.as_str() {
        "button" => {
            if let Ok(css) = std::fs::read_to_string("examples/button.tcss") {
                stylesheet.extend(&StyleSheet::parse(&css));
            }
        }
        _ => {}
    }
    let _guard = set_style_context(stylesheet);

    let widget = match demo.as_str() {
        "button" => build_button_demo(),
        _ => build_button_demo(),
    };

    let mut console = Console::new_with_record();
    {
        let options = console.options_mut();
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;
        console.sync_from_options();
    }

    let renderable = WidgetRenderable::new(&widget);
    console
        .print(&renderable, None, None, None, false, "")
        .unwrap();
    console
        .save_svg(
            out.to_string_lossy().as_ref(),
            &title,
            None,
            true,
            0.61,
            None,
        )
        .unwrap();

    Ok(())
}
