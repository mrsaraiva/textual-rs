/// Port of Python Textual `docs/examples/styles/opacity.py`.
///
/// Demonstrates the `opacity` CSS property at 0%, 25%, 50%, 75%, and 100%.
/// Five Labels with distinct ids, each showing a different opacity level.
///
/// Framework gap: `opacity` CSS property may not be fully supported in textual-rs;
/// included verbatim per port rules.
use textual::prelude::*;

const CSS: &str = r##"
#zero-opacity {
    opacity: 0%;
}

#quarter-opacity {
    opacity: 25%;
}

#half-opacity {
    opacity: 50%;
}

#three-quarter-opacity {
    opacity: 75%;
}

#full-opacity {
    opacity: 100%;
}

Screen {
    background: black;
}

Label {
    width: 100%;
    height: 1fr;
    border: outer dodgerblue;
    background: lightseagreen;
    content-align: center middle;
    text-style: bold;
}
"##;

struct OpacityApp;

impl TextualApp for OpacityApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("opacity: 0%").with_id("zero-opacity"))
            .with_child(Label::new("opacity: 25%").with_id("quarter-opacity"))
            .with_child(Label::new("opacity: 50%").with_id("half-opacity"))
            .with_child(Label::new("opacity: 75%").with_id("three-quarter-opacity"))
            .with_child(Label::new("opacity: 100%").with_id("full-opacity"))
    }
}

fn main() -> Result<()> {
    run_sync(OpacityApp)
}
