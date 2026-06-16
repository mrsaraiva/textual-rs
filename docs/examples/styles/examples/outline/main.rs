/// Port of Python Textual `docs/examples/styles/outline.py`.
///
/// Demonstrates the `outline` CSS property with a semi-transparent background
/// on a Label widget.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Screen {
    background: white;
    color: black;
}

Label {
    margin: 4 8;
    background: green 20%;
    outline: wide green;
    width: 100%;
}
"##;

struct OutlineApp;

impl TextualApp for OutlineApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(OutlineApp)
}
