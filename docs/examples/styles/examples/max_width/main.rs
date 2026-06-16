/// Port of Python Textual `docs/examples/styles/max_width.py`.
///
/// Demonstrates various max-width value types: h units, cells, percent, fixed cells.
use textual::compose;
use textual::prelude::*;

const CSS: &str = r##"
Horizontal {
    height: 100%;
    width: 100%;
}

Placeholder {
    width: 100%;
    height: 1fr;
}

#p1 {
    max-width: 50h;
}

#p2 {
    max-width: 999;
}

#p3 {
    max-width: 50%;
}

#p4 {
    max-width: 30;
}
"##;

struct MaxWidthApp;

impl TextualApp for MaxWidthApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            VerticalScroll::new().with_compose(compose![
                Node::new(Placeholder::new("max-width: 50h")).id("p1"),
                Node::new(Placeholder::new("max-width: 999")).id("p2"),
                Node::new(Placeholder::new("max-width: 50%")).id("p3"),
                Node::new(Placeholder::new("max-width: 30")).id("p4"),
            ]),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MaxWidthApp)
}
