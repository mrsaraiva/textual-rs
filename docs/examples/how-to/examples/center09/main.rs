/// Port of Python Textual `docs/examples/how-to/center09.py`.
///
/// Demonstrates centering with `align: center middle` on Screen.
/// Two `Static` widgets with class `words` are centered on the screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

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

fn main() -> textual::Result<()> {
    run_sync(CenterApp)
}
