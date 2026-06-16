/// Port of Python Textual `docs/examples/guide/layout/grid_layout_auto.py`.
///
/// Demonstrates a grid layout with `auto` column sizing alongside `1fr` columns.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3;
    grid-columns: auto 1fr 1fr;
    grid-rows: 25% 75%;
}

.box {
    height: 100%;
    border: solid green;
}
"##;

struct GridLayoutExample;

impl TextualApp for GridLayoutExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("First column").class("box"))
            .with_child(Static::new("Two").class("box"))
            .with_child(Static::new("Three").class("box"))
            .with_child(Static::new("Four").class("box"))
            .with_child(Static::new("Five").class("box"))
            .with_child(Static::new("Six").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(GridLayoutExample)
}
