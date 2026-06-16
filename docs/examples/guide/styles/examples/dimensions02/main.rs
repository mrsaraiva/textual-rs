/// Port of Python Textual `docs/examples/guide/styles/dimensions02.py`.
///
/// Demonstrates `width`, `height: auto`, and `background` set on a Static widget.
/// Python uses `on_mount` to set styles imperatively; ported here via CSS for
/// framework purity (same visual result).
use textual::prelude::*;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Static {
    background: purple;
    width: 30;
    height: auto;
}
"##;

struct DimensionsApp;

impl TextualApp for DimensionsApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(DimensionsApp)
}
