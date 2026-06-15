/// Port of Python Textual `docs/examples/widgets/digits.py`.
///
/// Demonstrates the `Digits` widget centered on screen with a double green border.
///
/// Python uses `#pi { border: double green; width: auto; }` (id selector).
/// Because the Rust `Digits` widget does not yet expose a `with_id()` builder
/// method, we target the widget via its type selector (`Digits { … }`), which
/// produces identical visual output for this single-widget app.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
Digits {
    border: double green;
    width: auto;
}
"#;

struct DigitApp;

impl TextualApp for DigitApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Digits::new("3.141,592,653,5897"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(DigitApp)
}
