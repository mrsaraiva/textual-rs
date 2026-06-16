/// Port of Python Textual `docs/examples/styles/height.py`.
///
/// Demonstrates the `height: 50%` CSS property on a generic widget.
/// Python uses `yield Widget()` — mapped to `Placeholder` as the closest
/// bare widget equivalent in textual-rs.
use textual::prelude::*;

const CSS: &str = r##"
Screen > Placeholder {
    background: green;
    height: 50%;
    color: white;
}
"##;

struct HeightApp;

impl TextualApp for HeightApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Placeholder::new(""))
    }
}

fn main() -> Result<()> {
    run_sync(HeightApp)
}
