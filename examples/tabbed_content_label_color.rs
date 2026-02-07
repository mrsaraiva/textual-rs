use textual::prelude::*;

fn build_widget() -> AppRoot {
    let tabs = TabbedContent::new()
        .with_pane(TabPane::new("Red", Label::new("Red!")).id("red"))
        .with_pane(TabPane::new("Green", Label::new("Green!")).id("green"));
    AppRoot::new().with_child(tabs)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new()?;
    if std::path::Path::new("examples/tabbed_content_label_color.tcss").exists() {
        app.watch_stylesheet(
            "examples/tabbed_content_label_color.tcss",
            std::time::Duration::from_millis(500),
        )?;
    }
    let mut root = build_widget();
    app.run_widget_tree(&mut root).await
}
