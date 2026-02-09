use textual::prelude::*;

struct TabbedContentLabelColorApp;

impl TextualApp for TabbedContentLabelColorApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            TabbedContent::new()
                .with_pane(TabPane::new("Red", Label::new("Red!")).id("red"))
                .with_pane(TabPane::new("Green", Label::new("Green!")).id("green")),
        )
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("examples/tabbed_content_label_color.tcss")
    }
}

fn main() -> Result<()> {
    run_sync(TabbedContentLabelColorApp)
}
