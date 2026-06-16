/// Port of Python Textual `docs/examples/guide/styles/dimensions01.py`.
///
/// Demonstrates setting widget dimensions and background color.
/// Python sets styles via `on_mount`; here we use an id-based CSS rule to
/// achieve the same result (background: purple, width: 30, height: 10).
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#widget {
    background: purple;
    width: 30;
    height: 10;
}
"##;

struct DimensionsApp;

impl TextualApp for DimensionsApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT).id("widget"))
    }
}

fn main() -> Result<()> {
    run_sync(DimensionsApp)
}
