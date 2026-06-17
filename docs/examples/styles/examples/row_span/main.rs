/// Port of Python Textual `docs/examples/styles/row_span.py`.
///
/// Demonstrates the `row-span` CSS property in a 4x4 grid with 7
/// Placeholder widgets spanning different numbers of rows.
///
/// Framework gap: `row-span` CSS property may not be fully implemented
/// in textual-rs.
use textual::prelude::*;

const CSS: &str = r##"
#p1 {
    row-span: 4;
}
#p2 {
    row-span: 3;
}
#p3 {
    row-span: 2;
}
#p4 {
    row-span: 1;  /* Didn't need to be set explicitly. */
}
#p5 {
    row-span: 3;
}
#p6 {
    row-span: 2;
}
#p7 {
    /* Default value is 1. */
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
                .with_child(Placeholder::new("").id("p1"))
                .with_child(Placeholder::new("").id("p2"))
                .with_child(Placeholder::new("").id("p3"))
                .with_child(Placeholder::new("").id("p4"))
                .with_child(Placeholder::new("").id("p5"))
                .with_child(Placeholder::new("").id("p6"))
                .with_child(Placeholder::new("").id("p7")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
