/// Port of Python Textual `docs/examples/how-to/center06.py`.
///
/// Demonstrates centering a widget with explicit width/height and text-align
/// on a screen using `align: center middle`.
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
    height: 9;
    text-align: center;
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
