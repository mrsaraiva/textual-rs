/// Port of Python Textual `docs/examples/styles/padding.py`.
///
/// Demonstrates the padding CSS property on a Label widget.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Screen {
    background: white;
    color: blue;
}

Label {
    padding: 4 8;
    background: blue 20%;
    width: 100%;
}
"##;

struct PaddingApp;

impl TextualApp for PaddingApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(PaddingApp)
}
