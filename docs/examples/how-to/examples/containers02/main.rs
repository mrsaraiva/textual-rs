/// Port of Python Textual `docs/examples/how-to/containers02.py`.
///
/// Demonstrates a `Vertical` container holding three fixed-size placeholder
/// widgets (`Box` in Python, using `Placeholder` here).
///
/// Python layout:
///   class Box(Placeholder):
///       DEFAULT_CSS = "Box { width: 16; height: 8; }"
///
///   class ContainerApp(App):
///       def compose(self):
///           with Vertical():
///               yield Box()
///               yield Box()
///               yield Box()
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
        let vertical = Vertical::new()
            .with_child(Placeholder::new(""))
            .with_child(Placeholder::new(""))
            .with_child(Placeholder::new(""));

        AppRoot::new().with_child(vertical)
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
