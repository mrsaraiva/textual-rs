/// Port of Python Textual `docs/examples/how-to/containers01.py`.
///
/// Demonstrates how to use the `Horizontal` container with `Placeholder`
/// widgets (equivalent to the Python `Box` subclass with fixed size CSS).
///
/// Python source:
///   - `Box(Placeholder)` with `DEFAULT_CSS = "Box { width: 16; height: 8; }"`
///   - `ContainerApp.compose()` yields `Horizontal` containing three `Box` widgets.
use textual::prelude::*;

const CSS: &str = r#"
Placeholder {
    width: 16;
    height: 8;
}
"#;

struct ContainerApp;

impl TextualApp for ContainerApp {
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

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
