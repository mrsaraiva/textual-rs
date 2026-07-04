/// Port of Python Textual `docs/examples/styles/min_height.py`.
///
/// Demonstrates `min-height` CSS property with four Placeholder widgets
/// inside a Horizontal container, each having a different min-height value.
use textual::prelude::*;

const CSS: &str = r##"
Horizontal {
    height: 100%;
    width: 100%;
    overflow-y: auto;
}

Placeholder {
    width: 1fr;
    height: 50%;
}

#p1 {
    min-height: 25%;  /* (1)! */
}

#p2 {
    min-height: 75%;
}

#p3 {
    min-height: 30;
}

#p4 {
    min-height: 40w;
}
"##;

struct MinHeightApp;

impl TextualApp for MinHeightApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(Placeholder::new("min-height: 25%").id("p1"))
                .with_child(Placeholder::new("min-height: 75%").id("p2"))
                .with_child(Placeholder::new("min-height: 30").id("p3"))
                .with_child(Placeholder::new("min-height: 40w").id("p4")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MinHeightApp)
}
