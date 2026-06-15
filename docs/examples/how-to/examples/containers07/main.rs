/// Port of Python Textual `docs/examples/how-to/containers07.py`.
///
/// Demonstrates a HorizontalScroll container with 10 Box (Placeholder) widgets,
/// each 16 wide x 8 tall, inside a heavy green border.
///
/// Python defines a `Box` subclass of `Placeholder` with fixed dimensions.
/// Rust uses a CSS class `box` applied to each Placeholder to achieve the
/// same sizing, since Rust does not support custom widget type selectors.
use textual::prelude::*;

const CSS: &str = r#"
.box {
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
        let mut horizontal = HorizontalScroll::new();
        for n in 0..10 {
            horizontal.push(Node::new(Placeholder::new(format!("Box {}", n + 1))).class("box"));
        }
        AppRoot::new().with_child(Node::new(horizontal).class("with-border"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
