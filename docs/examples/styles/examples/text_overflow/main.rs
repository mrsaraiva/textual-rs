/// Port of Python Textual `docs/examples/styles/text_overflow.py`.
///
/// Demonstrates `text-overflow` CSS property with three Static widgets:
/// - #static1: `text-overflow: clip`   — text is clipped at the boundary
/// - #static2: `text-overflow: fold`   — text wraps onto the next line
/// - #static3: `text-overflow: ellipsis` — text is truncated with "…"
///
/// All three widgets use `text-wrap: nowrap` so long text hits the overflow
/// boundary.
///
/// Framework gap: `text-overflow: clip/fold/ellipsis` and `text-wrap: nowrap`
/// CSS properties may not be fully rendered yet.
use textual::prelude::*;

const TEXT: &str = "I must not fear. Fear is the mind-killer. Fear is the little-death that brings total obliteration. I will face my fear.";

const CSS: &str = r##"
Static {
    height: 1fr;
    text-wrap: nowrap;
}

#static1 {
    text-overflow: clip;
    background: red 20%;
}
#static2 {
    text-overflow: fold;
    background: green 20%;
}
#static3 {
    text-overflow: ellipsis;
    background: blue 20%;
}
"##;

struct WrapApp;

impl TextualApp for WrapApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new(TEXT).id("static1"))
            .with_child(Static::new(TEXT).id("static2"))
            .with_child(Static::new(TEXT).id("static3"))
    }
}

fn main() -> Result<()> {
    run_sync(WrapApp)
}
