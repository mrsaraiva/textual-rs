/// Port of Python Textual `docs/examples/how-to/center01.py`.
///
/// Demonstrates how to center a widget on screen using `align: center middle`
/// on the Screen CSS rule.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(CenterApp)
}
