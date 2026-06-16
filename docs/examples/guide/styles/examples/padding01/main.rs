/// Port of Python Textual `docs/examples/guide/styles/padding01.py`.
///
/// Demonstrates padding on a Static widget. Python sets styles via on_mount;
/// ported here as inline CSS since Rust has no on_mount dynamic style setter.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#widget {
    background: purple;
    width: 30;
    padding: 2;
}
"##;

struct PaddingApp;

impl TextualApp for PaddingApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT).id("widget"))
    }
}

fn main() -> Result<()> {
    run_sync(PaddingApp)
}
