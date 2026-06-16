/// Port of Python Textual `docs/examples/styles/grid_gutter.py`.
///
/// Demonstrates `grid-gutter` with 8 labels arranged in a 2-column,
/// 4-row grid that has 1-row and 2-column gutters between cells.
///
/// Framework gap: `grid-gutter` rendering support may vary.
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 2 4;
    grid-gutter: 1 2;
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
                .with_child(Label::new("5"))
                .with_child(Label::new("6"))
                .with_child(Label::new("7"))
                .with_child(Label::new("8")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
