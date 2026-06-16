/// Port of Python Textual `docs/examples/styles/grid.py`.
///
/// Demonstrates CSS grid layout with `grid-size`, `row-span`, `column-span`,
/// and `tint` properties. Framework gaps: `tint`, `row-span`, `column-span`
/// may not be fully supported yet.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: grid;
    grid-size: 3 4;
    grid-rows: 1fr;
    grid-columns: 1fr;
    grid-gutter: 1;
}

Static {
    color: auto;
    background: lightblue;
    height: 100%;
    padding: 1 2;
}

#static1 {
    tint: magenta 40%;
    row-span: 3;
    column-span: 2;
}
"##;

struct GridApp;

impl TextualApp for GridApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("Grid cell 1\n\nrow-span: 3;\ncolumn-span: 2;").id("static1"))
            .with_child(Static::new("Grid cell 2").id("static2"))
            .with_child(Static::new("Grid cell 3").id("static3"))
            .with_child(Static::new("Grid cell 4").id("static4"))
            .with_child(Static::new("Grid cell 5").id("static5"))
            .with_child(Static::new("Grid cell 6").id("static6"))
            .with_child(Static::new("Grid cell 7").id("static7"))
    }
}

fn main() -> Result<()> {
    run_sync(GridApp)
}
