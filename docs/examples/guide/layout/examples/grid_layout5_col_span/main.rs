/// Port of Python Textual `docs/examples/guide/layout/grid_layout5_col_span.py`.
///
/// Demonstrates column spans in a grid layout.
/// A 3-column grid where the second widget spans 2 columns.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3;
}

#two {
    column-span: 2;
    tint: magenta 40%;
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
            .with_child(Static::new("Two [b](column-span: 2)").class("box").id("two"))
            .with_child(Static::new("Three").class("box"))
            .with_child(Static::new("Four").class("box"))
            .with_child(Static::new("Five").class("box"))
            .with_child(Static::new("Six").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(GridLayoutExample)
}
