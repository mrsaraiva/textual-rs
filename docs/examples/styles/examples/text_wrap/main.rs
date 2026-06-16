/// Port of Python Textual `docs/examples/styles/text_wrap.py`.
///
/// Demonstrates `text-wrap: wrap` (default) vs `text-wrap: nowrap` on Static widgets.
use textual::prelude::*;

const TEXT: &str = "I must not fear. Fear is the mind-killer. Fear is the little-death that brings total obliteration. I will face my fear.";

const CSS: &str = r##"
Static {
    height: 1fr;
}

#static1 {
    text-wrap: wrap; /* this is the default */
    background: blue 20%;
}
#static2 {
    text-wrap: nowrap; /* disable wrapping */
    background: green 20%;
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
    }
}

fn main() -> Result<()> {
    run_sync(WrapApp)
}
