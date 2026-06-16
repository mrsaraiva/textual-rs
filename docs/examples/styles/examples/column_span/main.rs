/// Port of Python Textual `docs/examples/styles/column_span.py`.
///
/// Demonstrates `column-span` in a 4x4 grid with 7 Placeholder widgets.
/// Framework gap: `column-span` CSS property may not be fully implemented.
use textual::prelude::*;

const CSS: &str = r##"
#p1 {
    column-span: 4;
}
#p2 {
    column-span: 3;
}
#p3 {
    column-span: 1;  /* Didn't need to be set explicitly. */
}
#p4 {
    column-span: 2;
}
#p5 {
    column-span: 2;
}
#p6 {
    /* Default value is 1. */
}
#p7 {
    column-span: 3;
}

Grid {
    grid-size: 4 4;
    grid-gutter: 1 2;
}

Placeholder {
    height: 100%;
}
"##;

struct MyApp;

impl TextualApp for MyApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(4, 4)
                .with_child(Node::new(Placeholder::new("")).id("p1"))
                .with_child(Node::new(Placeholder::new("")).id("p2"))
                .with_child(Node::new(Placeholder::new("")).id("p3"))
                .with_child(Node::new(Placeholder::new("")).id("p4"))
                .with_child(Node::new(Placeholder::new("")).id("p5"))
                .with_child(Node::new(Placeholder::new("")).id("p6"))
                .with_child(Node::new(Placeholder::new("")).id("p7")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
