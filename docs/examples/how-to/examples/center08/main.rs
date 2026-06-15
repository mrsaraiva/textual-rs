/// Port of Python Textual `docs/examples/how-to/center08.py`.
///
/// Demonstrates how `width: auto` makes a widget shrink to fit its content.
/// Two Static widgets with class `words` each display text with a blue
/// semi-transparent background and a wide white border, sized to their content.
use textual::prelude::*;

const CSS: &str = r#"
.words {
    background: blue 50%;
    border: wide white;
    width: auto;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("How about a nice game").class("words"))
            .with_child(Static::new("of chess?").class("words"))
    }
}

fn main() -> Result<()> {
    run_sync(CenterApp)
}
