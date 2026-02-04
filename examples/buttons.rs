use textual::demo_snapshot::{SnapshotArgs, snapshot_widget};
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
