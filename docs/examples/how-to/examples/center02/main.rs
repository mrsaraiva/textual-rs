/// Port of Python Textual `docs/examples/how-to/center02.py`.
///
/// Demonstrates centering a widget on screen using `align: center middle` on
/// Screen, with a blue semi-transparent background and wide white border on
/// the `#hello` Static widget.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

#hello {
    background: blue 50%;
    border: wide white;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!").id("hello"))
    }
}

fn main() -> Result<()> {
    run_sync(CenterApp)
}
