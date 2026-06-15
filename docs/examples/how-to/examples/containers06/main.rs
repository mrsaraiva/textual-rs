/// Port of Python Textual `docs/examples/how-to/containers06.py`.
///
/// Demonstrates a Horizontal container with 10 Box (Placeholder) widgets,
/// each 16 wide x 8 tall, inside a heavy green border.
///
/// Python defines a `Box` subclass of `Placeholder` with fixed dimensions.
/// Rust uses a CSS class `box` applied to each Placeholder to achieve the
/// same sizing, since Rust does not support custom widget type selectors.
///
/// Known framework gap: when child widgets overflow the border container
/// horizontally (10 x 16 = 160 > viewport ~122), the right border character
/// is not painted in the rows occupied by the children. This is a rendering
/// bug in the Rust framework; the Python output correctly clips children to
/// the border container's content area and always paints both border edges.
use textual::prelude::*;

const CSS: &str = r#"
.box {
    width: 16;
    height: 8;
}

.with-border {
    border: heavy green;
    width: 1fr;
    height: 1fr;
    layout: horizontal;
    overflow: hidden;
}
"#;

struct ContainerApp;

impl TextualApp for ContainerApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let mut horizontal = Horizontal::new();
        for n in 0..10 {
            horizontal.push(Node::new(Placeholder::new(format!("Box {}", n + 1))).class("box"));
        }
        AppRoot::new().with_child(Node::new(horizontal).class("with-border"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
