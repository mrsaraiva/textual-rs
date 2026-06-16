/// Port of Python Textual `docs/examples/guide/styles/border01.py`.
///
/// Demonstrates setting background, width, and border on a Label via
/// inline CSS (Python sets these via `on_mount` dynamic style assignment).
use textual::prelude::*;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Label {
    background: darkblue;
    width: 50%;
    border: heavy yellow;
}
"##;

struct BorderApp;

impl TextualApp for BorderApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(BorderApp)
}
