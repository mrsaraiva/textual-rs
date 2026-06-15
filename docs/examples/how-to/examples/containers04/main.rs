/// Port of Python Textual `docs/examples/how-to/containers04.py`.
///
/// Demonstrates `Horizontal` containers with a CSS class:
/// - Two `Horizontal` rows, each containing three `Placeholder` widgets
///   (`Box` in Python) with fixed 16×8 dimensions.
/// - The `.with-border` class applies a heavy green border.
use textual::prelude::*;

const CSS: &str = r#"
Placeholder {
    width: 16;
    height: 8;
}

.with-border {
    border: heavy green;
}
"#;

struct ContainerApp;

impl TextualApp for ContainerApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let row1 = Node::new(
            Horizontal::new()
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new("")),
        )
        .class("with-border");

        let row2 = Node::new(
            Horizontal::new()
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new("")),
        )
        .class("with-border");

        AppRoot::new().with_child(row1).with_child(row2)
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
