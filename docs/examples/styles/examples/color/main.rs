/// Port of Python Textual `docs/examples/styles/color.py`.
///
/// Demonstrates the `color` CSS property using named color, rgb(), and hsl()
/// across three Label widgets with 1fr height each.
use textual::prelude::*;

const CSS: &str = r##"
Label {
    height: 1fr;
    content-align: center middle;
    width: 100%;
}

#label1 {
    color: red;
}

#label2 {
    color: rgb(0, 255, 0);
}

#label3 {
    color: hsl(240, 100%, 50%);
}
"##;

struct ColorApp;

impl TextualApp for ColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("I'm red!").id("label1"))
            .with_child(Label::new("I'm rgb(0, 255, 0)!").id("label2"))
            .with_child(Label::new("I'm hsl(240, 100%, 50%)!").id("label3"))
    }
}

fn main() -> Result<()> {
    run_sync(ColorApp)
}
