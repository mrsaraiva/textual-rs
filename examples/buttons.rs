use textual::prelude::*;

fn build_buttons_widget() -> ScrollView {
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

fn maybe_snapshot() -> Option<(String, usize, usize)> {
    let mut args = std::env::args().skip(1);
    let mut snapshot: Option<String> = None;
    let mut width: usize = 120;
    let mut height: usize = 30;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--snapshot" => snapshot = args.next(),
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
            _ => {}
        }
    }
    snapshot.map(|path| (path, width, height))
}

fn render_svg_snapshot(path: &str, width: usize, height: usize) -> Result<()> {
    use rich_rs::Console;
    use textual::widget::{
        StyleSheet, WidgetRenderable, default_widget_stylesheet, set_style_context,
    };

    let mut stylesheet = default_widget_stylesheet();
    if let Ok(css) = std::fs::read_to_string("examples/button.tcss") {
        stylesheet.extend(&StyleSheet::parse(&css));
    }
    let _guard = set_style_context(stylesheet);

    let widget = build_buttons_widget();
    let mut console = Console::new_with_record();
    {
        let options = console.options_mut();
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;
        console.sync_from_options();
    }

    let renderable = WidgetRenderable::new(&widget);
    console.print(&renderable, None, None, None, false, "")?;
    console.save_svg(path, "textual-rs buttons", None, true, 0.61, None)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    if let Some((path, width, height)) = maybe_snapshot() {
        return render_svg_snapshot(&path, width, height);
    }

    let mut app = App::new()?;
    if std::path::Path::new("examples/button.tcss").exists() {
        app.watch_stylesheet(
            "examples/button.tcss",
            std::time::Duration::from_millis(500),
        )?;
    }

    let mut scroll_root = build_buttons_widget();
    app.run_widget_tree(&mut scroll_root).await
}
