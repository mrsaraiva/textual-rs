/// Port of Python Textual `docs/examples/styles/grid_size_both.py`.
///
/// Demonstrates `grid-size` with both column and row counts specified
/// (`grid-size: 2 4` = 2 columns, 4 rows) in a Grid with 5 Labels.
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 2 4;
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
            Grid::new(2, 4)
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
