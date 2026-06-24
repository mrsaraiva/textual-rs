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
        // id/class go on each Placeholder directly (Python
        // `Placeholder(id="foo")` / `Placeholder(classes="hidden")`) so the
        // `#foo`/`#bar`/`#baz` span rules AND the `Placeholder { height: 1fr }`
        // rule resolve on the SAME node, and the default label derives from the
        // id (`#foo`) — matching Python's `label or f"#{id}"`. A transparent
        // `Node` wrapper would split id/type across two nodes, leaving the
        // Placeholder label empty (rendering literal "Placeholder").
        AppRoot::new().with_child(
            Grid::new(3, 3)
                .with_child(Placeholder::new("").id("foo"))
                .with_child(Placeholder::new("").id("bar"))
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new("").class("hidden"))
                .with_child(Placeholder::new("").id("baz")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(KeylineApp)
}
