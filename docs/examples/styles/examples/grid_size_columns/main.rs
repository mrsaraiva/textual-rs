/// Port of Python Textual `docs/examples/styles/grid_size_columns.py`.
///
/// Demonstrates the `grid-size` CSS property to control column count.
/// Five labels are arranged in a 2-column grid (3 rows × 2 cols).
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 2;
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
            Grid::new(3, 2)
                .with_child(Label::new("1"))
                .with_child(Label::new("2"))
                .with_child(Label::new("3"))
                .with_child(Label::new("4"))
                .with_child(Label::new("5")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
