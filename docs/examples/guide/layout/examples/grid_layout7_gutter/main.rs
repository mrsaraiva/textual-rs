/// Port of Python Textual `docs/examples/guide/layout/grid_layout7_gutter.py`.
///
/// Demonstrates a 3-column grid layout with gutter spacing.
/// Six Static widgets with class `box` are placed in the grid.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3;
    grid-gutter: 1;
    background: lightgreen;
}

.box {
    background: darkmagenta;
    height: 100%;
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
            .with_child(Static::new("One").class("box"))
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
