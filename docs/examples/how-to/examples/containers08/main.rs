/// Port of Python Textual `docs/examples/how-to/containers08.py`.
///
/// Demonstrates `Center` and `Right` containers with a `Placeholder` widget
/// (`Box` in Python) with fixed 16×5 dimensions.
///
/// Python defines a `Box` subclass of `Placeholder` with fixed dimensions.
/// Rust uses a CSS class `box` applied to each Placeholder to achieve the
/// same sizing, since Rust does not support custom widget type selectors.
///
/// Layout:
///   - Box 1 — raw, no container
///   - Center container (with-border) containing Box 2
///   - Right container (with-border) containing Box 3
use textual::prelude::*;

const CSS: &str = r#"
.box {
    width: 16;
    height: 5;
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
        AppRoot::new()
            .with_child(Node::new(Placeholder::new("Box 1")).class("box"))
            .with_child(
                Node::new(
                    Center::new()
                        .with_child(Node::new(Placeholder::new("Box 2")).class("box")),
                )
                .class("with-border"),
            )
            .with_child(
                Node::new(
                    Right::new()
                        .with_child(Node::new(Placeholder::new("Box 3")).class("box")),
                )
                .class("with-border"),
            )
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
