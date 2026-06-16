/// Port of Python Textual `docs/examples/guide/styles/outline01.py`.
///
/// Demonstrates the `outline` style: a Static widget with a heavy yellow
/// outline, darkblue background, and 50% width.
///
/// Python sets these in `on_mount` via `self.widget.styles.*`; here they
/// are expressed as inline CSS loaded at configure time.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#widget {
    background: darkblue;
    width: 50%;
    outline: heavy yellow;
}
"##;

struct OutlineApp;

impl TextualApp for OutlineApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT).id("widget"))
    }
}

fn main() -> Result<()> {
    run_sync(OutlineApp)
}
