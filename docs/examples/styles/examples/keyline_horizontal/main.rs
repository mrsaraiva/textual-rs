/// Port of Python Textual `docs/examples/styles/keyline_horizontal.py`.
///
/// Demonstrates `keyline` CSS property on a Horizontal container.
/// Three Placeholder widgets inside a Horizontal with `keyline: thin $secondary`.
///
/// Note: `keyline` is not yet implemented in textual-rs (framework gap).
use textual::prelude::*;

const CSS: &str = r##"
Placeholder {
    margin: 1;
    width: 1fr;
}

Horizontal {
    keyline: thin $secondary;
}
"##;

struct KeylineApp;

impl TextualApp for KeylineApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new("")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(KeylineApp)
}
