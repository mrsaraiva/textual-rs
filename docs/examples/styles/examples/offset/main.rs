/// Port of Python Textual `docs/examples/styles/offset.py`.
///
/// Demonstrates the `offset` CSS property applied to Label widgets.
/// Note: `offset` support in textual-rs may be partial (framework-gap flag).
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    background: white;
    color: black;
    layout: horizontal;
}
Label {
    width: 20;
    height: 10;
    content-align: center middle;
}

.paul {
    offset: 8 2;
    background: red 20%;
    border: outer red;
    color: red;
}

.duncan {
    offset: 4 10;
    background: green 20%;
    border: outer green;
    color: green;
}

.chani {
    offset: 0 -3;
    background: blue 20%;
    border: outer blue;
    color: blue;
}
"##;

struct OffsetApp;

impl TextualApp for OffsetApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Paul (offset 8 2)").class("paul"))
            .with_child(Label::new("Duncan (offset 4 10)").class("duncan"))
            .with_child(Label::new("Chani (offset 0 -3)").class("chani"))
    }
}

fn main() -> Result<()> {
    run_sync(OffsetApp)
}
