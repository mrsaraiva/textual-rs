/// Port of Python Textual `docs/examples/styles/border.py`.
///
/// Demonstrates `border` CSS property with solid/dashed/tall border types
/// on three Label widgets.
use textual::prelude::*;

const CSS: &str = r##"
#label1 {
    background: red 20%;
    color: red;
    border: solid red;
}

#label2 {
    background: green 20%;
    color: green;
    border: dashed green;
}

#label3 {
    background: blue 20%;
    color: blue;
    border: tall blue;
}

Screen {
    background: white;
}

Screen > Label {
    width: 100%;
    height: 5;
    content-align: center middle;
    color: white;
    margin: 1;
    box-sizing: border-box;
}
"##;

struct BorderApp;

impl TextualApp for BorderApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("My border is solid red").with_id("label1"))
            .with_child(Label::new("My border is dashed green").with_id("label2"))
            .with_child(Label::new("My border is tall blue").with_id("label3"))
    }
}

fn main() -> Result<()> {
    run_sync(BorderApp)
}
