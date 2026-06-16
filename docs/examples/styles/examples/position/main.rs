/// Port of Python Textual `docs/examples/styles/position.py`.
///
/// Demonstrates absolute and relative `position` CSS properties with `offset`.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

Label {
    padding: 1;
    background: $panel;
    border: thick $border;
}

Label#label1 {
    position: absolute;
    offset: 2 1;
}

Label#label2 {
    position: relative;
    offset: 2 1;
}
"##;

struct PositionApp;

impl TextualApp for PositionApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Absolute").with_id("label1"))
            .with_child(Label::new("Relative").with_id("label2"))
    }
}

fn main() -> Result<()> {
    run_sync(PositionApp)
}
