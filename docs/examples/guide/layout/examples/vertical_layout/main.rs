/// Port of Python Textual `docs/examples/guide/layout/vertical_layout.py`.
///
/// Demonstrates vertical layout with three boxes sharing equal height.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: vertical;
}

.box {
    height: 1fr;
    border: solid green;
}
"##;

struct VerticalLayoutExample;

impl TextualApp for VerticalLayoutExample {
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
    run_sync(VerticalLayoutExample)
}
