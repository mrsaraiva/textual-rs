/// Port of Python Textual `docs/examples/guide/styles/screen.py`.
///
/// Demonstrates setting background and border on the Screen via CSS
/// (Python sets these via `on_mount` dynamic style assignment on `self.screen`).
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    background: darkblue;
    border: heavy white;
}
"##;

struct ScreenApp;

impl TextualApp for ScreenApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(ScreenApp)
}
