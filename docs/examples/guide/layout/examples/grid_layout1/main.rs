/// Port of Python Textual `docs/examples/guide/layout/grid_layout1.py`.
///
/// Demonstrates a 3x2 grid layout with six Static widgets,
/// each styled with full height and a solid green border.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3 2;
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
    }
}

fn main() -> Result<()> {
    run_sync(GridLayoutExample)
}
