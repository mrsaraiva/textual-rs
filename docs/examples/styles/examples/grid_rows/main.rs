/// Port of Python Textual `docs/examples/styles/grid_rows.py`.
///
/// Demonstrates the grid-rows CSS property with a 2-column, 5-row grid.
/// Each row uses a different sizing: 1fr, fixed (6), percentage (25%).
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 2 5;
    grid-rows: 1fr 6 25%;
}

Label {
    border: round white;
    content-align: center middle;
    width: 100%;
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
            Grid::new(5, 2)
                .with_child(Label::new("1fr"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("height = 6"))
                .with_child(Label::new("height = 6"))
                .with_child(Label::new("25%"))
                .with_child(Label::new("25%"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("height = 6"))
                .with_child(Label::new("height = 6")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
