/// Port of Python Textual `docs/examples/guide/layout/horizontal_layout_overflow.py`.
///
/// Demonstrates horizontal layout with overflow-x: auto so widgets scroll
/// horizontally when they exceed the screen width.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: horizontal;
    overflow-x: auto;
}

.box {
    height: 100%;
    border: solid green;
}
"##;

struct HorizontalLayoutExample;

impl TextualApp for HorizontalLayoutExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("One").class("box"))
            .with_child(Static::new("Two").class("box"))
            .with_child(Static::new("Three").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(HorizontalLayoutExample)
}
