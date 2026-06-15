/// Port of Python Textual `docs/examples/how-to/center04.py`.
///
/// Demonstrates centering a widget on screen using `align: center middle` on
/// Screen, with a blue semi-transparent background, wide white border, and
/// fixed `width: 40` on the `#hello` Static widget.
use textual::prelude::*;

const QUOTE: &str = "Could not find you in Seattle and no terminal is in operation at your classified address.";

const CSS: &str = r#"
Screen {
    align: center middle;
}

#hello {
    background: blue 50%;
    border: wide white;
    width: 40;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(QUOTE).id("hello"))
    }
}

fn main() -> Result<()> {
    run_sync(CenterApp)
}
