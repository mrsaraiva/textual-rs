/// Port of Python Textual `docs/examples/guide/styles/colors.py`.
///
/// Demonstrates setting background and border styles on a Static widget.
/// Python sets these via `on_mount` using inline style mutation;
/// here we use CSS to achieve the same visual result (framework gap: no runtime
/// inline-style mutation API exposed to TextualApp).
use textual::prelude::*;

const CSS: &str = r##"
#textual {
    background: darkblue;
    border: heavy white;
}
"##;

struct WidgetApp;

impl TextualApp for WidgetApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Textual").id("textual"))
    }
}

fn main() -> Result<()> {
    run_sync(WidgetApp)
}
