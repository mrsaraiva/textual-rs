/// Port of Python Textual `docs/examples/guide/layout/grid_layout2.py`.
///
/// Demonstrates a 3-column grid layout with seven Static widgets that each
/// fill 100% height and have a solid green border.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3;
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
            .with_child(Static::new("One").class("box"))
            .with_child(Static::new("Two").class("box"))
            .with_child(Static::new("Three").class("box"))
            .with_child(Static::new("Four").class("box"))
            .with_child(Static::new("Five").class("box"))
            .with_child(Static::new("Six").class("box"))
            .with_child(Static::new("Seven").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(GridLayoutExample)
}
