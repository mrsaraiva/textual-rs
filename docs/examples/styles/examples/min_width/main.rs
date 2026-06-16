/// Port of Python Textual `docs/examples/styles/min_width.py`.
///
/// Demonstrates `min-width` CSS property with various unit types:
/// percentage, cells, and viewport-relative (h) units.
use textual::prelude::*;

const CSS: &str = r##"
VerticalScroll {
    height: 100%;
    width: 100%;
    overflow-x: auto;
}

Placeholder {
    height: 1fr;
    width: 50%;
}

#p1 {
    min-width: 25%;
}

#p2 {
    min-width: 75%;
}

#p3 {
    min-width: 100;
}

#p4 {
    min-width: 400h;
}
"##;

struct MinWidthApp;

impl TextualApp for MinWidthApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            VerticalScroll::new()
                .with_child(Placeholder::new("min-width: 25%").id("p1"))
                .with_child(Placeholder::new("min-width: 75%").id("p2"))
                .with_child(Placeholder::new("min-width: 100").id("p3"))
                .with_child(Placeholder::new("min-width: 400h").id("p4")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MinWidthApp)
}
