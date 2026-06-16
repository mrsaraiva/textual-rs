/// Port of Python Textual `docs/examples/styles/layout.py`.
///
/// Demonstrates vertical and horizontal layout on two Container widgets.
use textual::prelude::*;

const CSS: &str = r##"
#vertical-layout {
    layout: vertical;
    background: darkmagenta;
    height: auto;
}

#horizontal-layout {
    layout: horizontal;
    background: darkcyan;
    height: auto;
}

Label {
    margin: 1;
    width: 12;
    color: black;
    background: yellowgreen;
}
"##;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Container::new()
                    .id("vertical-layout")
                    .with_child(Label::new("Layout"))
                    .with_child(Label::new("Is"))
                    .with_child(Label::new("Vertical")),
            )
            .with_child(
                Container::new()
                    .id("horizontal-layout")
                    .with_child(Label::new("Layout"))
                    .with_child(Label::new("Is"))
                    .with_child(Label::new("Horizontal")),
            )
    }
}

fn main() -> Result<()> {
    run_sync(LayoutApp)
}
