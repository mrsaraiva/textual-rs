/// Port of Python Textual `docs/examples/guide/layout/vertical_layout_scrolled.py`.
///
/// Demonstrates a vertical layout with three tall Static widgets
/// (each 14 rows) that cause the screen to scroll.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: vertical;
}

.box {
    height: 14;
    border: solid green;
}
"##;

struct VerticalLayoutScrolledExample;

impl TextualApp for VerticalLayoutScrolledExample {
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
    run_sync(VerticalLayoutScrolledExample)
}
