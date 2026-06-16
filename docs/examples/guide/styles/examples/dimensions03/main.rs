/// Port of Python Textual `docs/examples/guide/styles/dimensions03.py`.
///
/// Demonstrates setting widget dimensions and background via inline styles
/// applied at mount time: background=purple, width=50%, height=80%.
///
/// Note: Python applies styles via on_mount(). In Rust we apply them via
/// inline CSS on the widget directly, which is the idiomatic equivalent.
use textual::prelude::*;

const CSS: &str = r##"
Static {
    background: purple;
    width: 50%;
    height: 80%;
}
"##;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.";

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
