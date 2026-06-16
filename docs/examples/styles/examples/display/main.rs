/// Port of Python Textual `docs/examples/styles/display.py`.
///
/// Demonstrates the `display: none` CSS property.
/// Three Static widgets are shown; the middle one (class "remove") has display:none.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    background: green;
}

Static {
    height: 5;
    background: white;
    color: blue;
    border: heavy blue;
}

Static.remove {
    display: none;
}
"##;

struct DisplayApp;

impl TextualApp for DisplayApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("Widget 1"))
            .with_child(Static::new("Widget 2").class("remove"))
            .with_child(Static::new("Widget 3"))
    }
}

fn main() -> Result<()> {
    run_sync(DisplayApp)
}
