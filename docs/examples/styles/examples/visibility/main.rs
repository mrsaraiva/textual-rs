/// Port of Python Textual `docs/examples/styles/visibility.py`.
///
/// Demonstrates the `visibility: hidden` CSS property. Widget 2 has class
/// `invisible` and is hidden but still occupies space in the layout.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    background: green;
}

Label {
    height: 5;
    width: 100%;
    background: white;
    color: blue;
    border: heavy blue;
}

Label.invisible {
    visibility: hidden;
}
"##;

struct VisibilityApp;

impl TextualApp for VisibilityApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Widget 1"))
            .with_child(Label::new("Widget 2").class("invisible"))
            .with_child(Label::new("Widget 3"))
    }
}

fn main() -> Result<()> {
    run_sync(VisibilityApp)
}
