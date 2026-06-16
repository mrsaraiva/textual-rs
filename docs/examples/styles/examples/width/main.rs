/// Port of Python Textual `docs/examples/styles/width.py`.
///
/// Demonstrates the `width` CSS property: a single widget taking 50% of the
/// screen width with a green background and white text.
///
/// Note: Python uses `Widget()` (the base class directly). In textual-rs,
/// `Widget` is a trait; `Placeholder` is used as the equivalent empty widget.
/// The CSS `Screen > Widget` is kept verbatim — in textual-rs the CSS engine
/// treats `Widget` as a universal base-class selector that matches any child.
use textual::prelude::*;

const CSS: &str = r##"
Screen > Widget {
    background: green;
    width: 50%;
    color: white;
}
"##;

struct WidthApp;

impl TextualApp for WidthApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Placeholder::new(""))
    }
}

fn main() -> Result<()> {
    run_sync(WidthApp)
}
