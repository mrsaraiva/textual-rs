use std::sync::{Arc, Mutex};

use rich_rs::{Segment, Segments};
use textual::demo_snapshot::{SnapshotArgs, snapshot_widget};
use textual::prelude::*;

struct StatusLine {
    id: WidgetId,
    text: Arc<Mutex<String>>,
}

impl StatusLine {
    fn new(text: Arc<Mutex<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            text,
        }
    }
}

impl Widget for StatusLine {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let text = self.text.lock().unwrap_or_else(|e| e.into_inner());
        let line = rich_rs::set_cell_size(&text, width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn build_buttons_widget() -> AppRoot {
    let status = Arc::new(Mutex::new(String::from("")));
    let status_clone = status.clone();

    let headers = Horizontal::new()
        .with_child(Node::new(Static::new("Standard Buttons")).class("header"))
        .with_child(Node::new(Static::new("Disabled Buttons")).class("header"))
        .with_child(Node::new(Static::new("Flat Buttons")).class("header"))
        .with_child(Node::new(Static::new("Disabled Flat Buttons")).class("header"));

    let buttons = Horizontal::new()
        .with_child(
            VerticalScroll::new()
                .with_child(Button::new("Default").on_press({
                    let status = status.clone();
                    move |button| {
                        *status.lock().unwrap() = button.describe();
                    }
                }))
                .with_child(Button::primary("Primary!").on_press({
                    let status = status.clone();
                    move |button| {
                        *status.lock().unwrap() = button.describe();
                    }
                }))
                .with_child(Button::success("Success!").on_press({
                    let status = status.clone();
                    move |button| {
                        *status.lock().unwrap() = button.describe();
                    }
                }))
                .with_child(Button::warning("Warning!").on_press({
                    let status = status.clone();
                    move |button| {
                        *status.lock().unwrap() = button.describe();
                    }
                }))
                .with_child(Button::error("Error!").on_press({
                    let status = status.clone();
                    move |button| {
                        *status.lock().unwrap() = button.describe();
                    }
                })),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Button::new("Default").disabled(true))
                .with_child(Button::primary("Primary!").disabled(true))
                .with_child(Button::success("Success!").disabled(true))
                .with_child(Button::warning("Warning!").disabled(true))
                .with_child(Button::error("Error!").disabled(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Button::new("Default").flat(true))
                .with_child(Button::primary("Primary!").flat(true))
                .with_child(Button::success("Success!").flat(true))
                .with_child(Button::warning("Warning!").flat(true))
                .with_child(Button::error("Error!").flat(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Button::new("Default").disabled(true).flat(true))
                .with_child(Button::primary("Primary!").disabled(true).flat(true))
                .with_child(Button::success("Success!").disabled(true).flat(true))
                .with_child(Button::warning("Warning!").disabled(true).flat(true))
                .with_child(Button::error("Error!").disabled(true).flat(true)),
        );

    let scroll = ScrollView::new(buttons).scroll_step(2);
    AppRoot::new()
        .with_child(headers)
        .with_child(StatusLine::new(status_clone))
        .with_child(scroll)
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    if let Some(args) = SnapshotArgs::parse() {
        let widget = build_buttons_widget();
        return snapshot_widget(
            &widget,
            &args,
            Some(std::path::Path::new("examples/button.tcss")),
        );
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
