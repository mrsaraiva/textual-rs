/// Port of Python Textual `docs/examples/styles/max_height.py`.
///
/// Demonstrates `max-height` with different units: viewport width (10w),
/// large fixed value (999), percentage (50%), and fixed cells (10).
use textual::prelude::*;

const CSS: &str = r##"
Horizontal {
    height: 100%;
    width: 100%;
}

Placeholder {
    height: 100%;
    width: 1fr;
}

#p1 {
    max-height: 10w;
}

#p2 {
    max-height: 999;  /* (1)! */
}

#p3 {
    max-height: 50%;
}

#p4 {
    max-height: 10;
}
"##;

struct MaxHeightApp;

impl TextualApp for MaxHeightApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(Placeholder::new("max-height: 10w").id("p1"))
                .with_child(Placeholder::new("max-height: 999").id("p2"))
                .with_child(Placeholder::new("max-height: 50%").id("p3"))
                .with_child(Placeholder::new("max-height: 10").id("p4")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MaxHeightApp)
}
