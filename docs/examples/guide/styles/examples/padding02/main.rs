/// Port of Python Textual `docs/examples/guide/styles/padding02.py`.
///
/// Demonstrates padding on a Static widget. Python sets the styles via
/// `on_mount` (background purple, width 30, padding (2, 4)); here we use
/// a CSS rule for the same visual result.
use textual::prelude::*;

const CSS: &str = r##"
Static {
    background: purple;
    width: 30;
    padding: 2 4;
}
"##;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

struct PaddingApp;

impl TextualApp for PaddingApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(PaddingApp)
}
