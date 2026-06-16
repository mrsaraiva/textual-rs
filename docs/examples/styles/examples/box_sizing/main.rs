/// Port of Python Textual `docs/examples/styles/box_sizing.py`.
///
/// Demonstrates `box-sizing: border-box` vs `box-sizing: content-box`
/// using two Static widgets with matching padding, border, margin, and height.
use textual::prelude::*;

const CSS: &str = r##"
#static1 {
    box-sizing: border-box;
}

#static2 {
    box-sizing: content-box;
}

Screen {
    background: white;
    color: black;
}

App Static {
    background: blue 20%;
    height: 5;
    margin: 2;
    padding: 1;
    border: wide black;
}
"##;

struct BoxSizingApp;

impl TextualApp for BoxSizingApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("I'm using border-box!").id("static1"))
            .with_child(Static::new("I'm using content-box!").id("static2"))
    }
}

fn main() -> Result<()> {
    run_sync(BoxSizingApp)
}
