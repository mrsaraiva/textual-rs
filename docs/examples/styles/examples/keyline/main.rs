/// Port of Python Textual `docs/examples/styles/keyline.py`.
///
/// Demonstrates the `keyline` CSS property with a 3x3 grid of Placeholders.
/// Framework gap: `keyline` CSS property may not be implemented in textual-rs.
/// Framework gap: `column-span` and `row-span` CSS properties may not be fully
/// implemented in textual-rs.
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 3 3;
    grid-gutter: 1;
    padding: 2 3;
    keyline: heavy green;
}
Placeholder {
    height: 1fr;
}
.hidden {
    visibility: hidden;
}
#foo {
    column-span: 2;
}
#bar {
    row-span: 2;
}
#baz {
    column-span:3;
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
            Grid::new(3, 3)
                .with_child(Node::new(Placeholder::new("")).id("foo"))
                .with_child(Node::new(Placeholder::new("")).id("bar"))
                .with_child(Placeholder::new(""))
                .with_child(Node::new(Placeholder::new("")).class("hidden"))
                .with_child(Node::new(Placeholder::new("")).id("baz")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(KeylineApp)
}
