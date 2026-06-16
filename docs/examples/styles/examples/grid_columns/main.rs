/// Port of Python Textual `docs/examples/styles/grid_columns.py`.
///
/// Demonstrates `grid-columns` with mixed fractional and fixed widths
/// (1fr, 16, 2fr) in a grid with 5 columns and 2 rows.
///
/// Framework gap: `grid-columns` CSS property with mixed fr/fixed values
/// may not be fully supported in textual-rs yet.
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 5 2;
    grid-columns: 1fr 16 2fr;
}

Label {
    border: round white;
    content-align-horizontal: center;
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
            Grid::new(2, 5)
                .with_child(Label::new("1fr"))
                .with_child(Label::new("width = 16"))
                .with_child(Label::new("2fr"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("width = 16"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("width = 16"))
                .with_child(Label::new("2fr"))
                .with_child(Label::new("1fr"))
                .with_child(Label::new("width = 16")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
