/// Port of Python Textual `docs/examples/styles/text_opacity.py`.
///
/// Demonstrates the `text-opacity` CSS property at five levels:
/// 0%, 25%, 50%, 75%, and 100%.
///
/// Framework gap: `text-opacity` CSS property may not be fully
/// supported in textual-rs yet.
use textual::prelude::*;

const CSS: &str = r##"
#zero-opacity {
    text-opacity: 0%;
}

#quarter-opacity {
    text-opacity: 25%;
}

#half-opacity {
    text-opacity: 50%;
}

#three-quarter-opacity {
    text-opacity: 75%;
}

#full-opacity {
    text-opacity: 100%;
}

Label {
    height: 1fr;
    width: 100%;
    text-align: center;
    text-style: bold;
}
"##;

struct TextOpacityApp;

impl TextualApp for TextOpacityApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("text-opacity: 0%").with_id("zero-opacity"))
            .with_child(Label::new("text-opacity: 25%").with_id("quarter-opacity"))
            .with_child(Label::new("text-opacity: 50%").with_id("half-opacity"))
            .with_child(Label::new("text-opacity: 75%").with_id("three-quarter-opacity"))
            .with_child(Label::new("text-opacity: 100%").with_id("full-opacity"))
    }
}

fn main() -> Result<()> {
    run_sync(TextOpacityApp)
}
